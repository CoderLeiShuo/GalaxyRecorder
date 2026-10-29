use crate::{
    connection::{client, endpoint, response_text, ConnectionState},
    network,
    protocol::authority,
};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};
use tauri::{AppHandle, State};

pub fn fields(text: &str) -> BTreeMap<String, String> {
    text.replace("\\r\\n", "\n")
        .replace("\\n", "\n")
        .replace('\r', "\n")
        .lines()
        .filter_map(|line| {
            line.split_once('=')
                .or_else(|| {
                    line.split_once(':').filter(|(key, value)| {
                        key.trim().eq_ignore_ascii_case("mic")
                            && matches!(value.trim(), "ON" | "OFF")
                    })
                })
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .filter(|(k, _)| !k.is_empty())
        .collect()
}
pub fn accepted(body: &str) -> bool {
    let body = body.replace("\\n", "\n");
    !body.to_ascii_lowercase().contains("<html")
        && !body.to_ascii_uppercase().contains("ERROR")
        && body.lines().any(|line| line.trim() == "OK")
}
async fn request(
    address: &str,
    action: &str,
    property: &str,
    value: Option<&str>,
) -> Result<String, String> {
    let mut url = endpoint(address, action, property)?;
    if let Some(value) = value {
        url.query_pairs_mut().append_pair("value", value);
    }
    let response = client()?
        .get(url)
        .timeout(Duration::from_secs(if action == "format" { 25 } else { 8 }))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    response_text(response, 128 * 1024).await
}
#[derive(Serialize)]
pub struct Status {
    pub fields: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}
#[tauri::command]
pub async fn recorder_status(
    app: AppHandle,
    address: String,
    diagnostics: State<'_, ConnectionState>,
) -> Result<Status, String> {
    network::prepare(&app, false).await?;
    let address = authority(&address)?;
    if diagnostics.family(&address) == "L6" {
        return l6_status(&address).await;
    }
    let mut status = Status {
        fields: BTreeMap::new(),
        warnings: Vec::new(),
    };
    for property in [
        "DVR",
        "Camera.Menu.HeartBeat",
        "Heart.Cmd.*",
        "Camera.Preview.MJPEG.status.*",
        "Camera.Menu.CardInfo.*",
        "Camera.Menu.SD0",
        "Camera.Menu.*",
        "Camera.Menu.FWversion",
        "MCUversion",
        "Camera.Menu.MCUversion",
        "mic",
    ] {
        let response = tokio::time::timeout(
            Duration::from_secs(if property.contains("MCU") || property == "DVR" {
                3
            } else {
                8
            }),
            request(&address, "get", property, None),
        )
        .await
        .unwrap_or_else(|_| Err("状态查询超时".into()));
        match response {
            Ok(body) => {
                let values = fields(&body);
                if values.is_empty() {
                    status.warnings.push(format!("{property} 未提供状态字段"));
                } else {
                    status.fields.extend(values);
                }
            }
            Err(_) => status.warnings.push(format!("{property} 读取失败")),
        }
    }
    diagnostics.log(
        "status",
        format!(
            "状态字段={}，未读项目={}",
            status.fields.len(),
            status.warnings.len()
        ),
    );
    if status.fields.is_empty() {
        return Err("设备未返回可识别状态，请复制连接诊断".into());
    }
    Ok(status)
}
pub fn command_args(
    command: &str,
    value: Option<&str>,
) -> Result<(&'static str, &'static str, Option<String>), String> {
    match command {
        "record" => Ok(("set", "Video", Some("normal".into()))),
        "stop" => Ok(("set", "Video", Some("stop".into()))),
        "capture" => Ok(("set", "Video", Some("capture".into()))),
        "event" => Ok(("set", "Video", Some("event".into()))),
        "mic" if matches!(value, Some("ON" | "OFF")) => {
            Ok(("set", "mic", value.map(str::to_string)))
        }
        "authTime" if matches!(value, Some("0" | "1" | "2")) => {
            Ok(("set", "AuthTime", value.map(str::to_string)))
        }
        "format" if value == Some("确认清空存储卡") => Ok(("format", "format", None)),
        _ => Err("不支持的设备操作或缺少确认".into()),
    }
}
#[tauri::command]
pub async fn recorder_command(
    app: AppHandle,
    address: String,
    command: String,
    value: Option<String>,
    diagnostics: State<'_, ConnectionState>,
) -> Result<(), String> {
    let (action, property, value) = command_args(&command, value.as_deref())?;
    if command == "format"
        && (crate::transfers::has_active(&app).await? || crate::editing::busy(&app))
    {
        return Err("请先完成或取消下载和导出，再格式化存储卡".into());
    }
    network::prepare(&app, false).await?;
    let address = authority(&address)?;
    if diagnostics.family(&address) == "L6" {
        return l6_command(&address, &command, value.as_deref()).await;
    }
    let result = request(&address, action, property, value.as_deref()).await?;
    diagnostics.log(
        "control",
        format!(
            "{command}：设备{}",
            if accepted(&result) {
                "已接受"
            } else {
                "未确认"
            }
        ),
    );
    if !accepted(&result) {
        return Err("记录仪未确认操作，请刷新状态后重试".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn open_media(
    app: AppHandle,
    address: Option<String>,
    mut video: crate::protocol::Video,
    server: State<'_, crate::media::MediaServer>,
) -> Result<crate::media::Source, String> {
    if video.saved_path.is_some() {
        video = crate::library::find(&app, &video.id).await?;
    }
    if video.saved_path.is_none() {
        network::prepare(&app, false).await?;
    }
    server.video(address.as_deref(), &video, &app).await
}
#[tauri::command]
pub async fn live_media(
    app: AppHandle,
    address: String,
    server: State<'_, crate::media::MediaServer>,
    diagnostics: State<'_, ConnectionState>,
) -> Result<crate::media::Source, String> {
    if cfg!(target_os = "ios") {
        return Err("iOS 测试版暂不支持 RTSP 实时预览，可在录像页回看和保存录像".into());
    }
    if diagnostics.family(&address) == "L6" {
        return Err("此方案的专用实时视频通道尚未接入，可播放与保存录像".into());
    }
    network::prepare(&app, false).await?;
    let address = authority(&address)?;
    let host = address.split(':').next().unwrap();
    let mut selected = None;
    for path in [
        "/liveRTSP/av0",
        "/liveRTSP/v1",
        "/liveRTSP/av1",
        "/liveRTSP/av2",
        "/liveRTSP/av4",
    ] {
        let url = format!("rtsp://{host}{path}");
        let result = tokio::time::timeout(Duration::from_millis(1500), async {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut stream = tokio::net::TcpStream::connect((host, 554)).await?;
            stream.write_all(format!("DESCRIBE {url} RTSP/1.0\r\nCSeq: 1\r\nAccept: application/sdp\r\nUser-Agent: GalaxyRecorder\r\n\r\n").as_bytes()).await?;
            let mut bytes = [0; 4096]; let count = stream.read(&mut bytes).await?;
            Ok::<_, std::io::Error>(String::from_utf8_lossy(&bytes[..count]).starts_with("RTSP/1.0 200"))
        }).await;
        if matches!(result, Ok(Ok(true))) {
            selected = Some(url);
            break;
        }
    }
    server
        .live(selected.unwrap_or_else(|| format!("rtsp://{host}/liveRTSP/av0")))
        .await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn microphone_colon_response_is_not_lost() {
        let parsed = fields("0\nmic:ON\nRecord=2\nOK\n");
        assert_eq!(parsed.get("mic").map(String::as_str), Some("ON"));
        assert_eq!(parsed.get("Record").map(String::as_str), Some("2"));
        assert!(fields("https://invalid\nOK").is_empty());
    }
    #[test]
    fn commands_are_bounded_and_format_requires_explicit_confirmation() {
        assert!(command_args("format", None).is_err());
        assert!(command_args("mic", Some("yes")).is_err());
        assert_eq!(
            command_args("capture", None).unwrap(),
            ("set", "Video", Some("capture".into()))
        );
        assert!(accepted("0\nOK\n"));
        assert!(!accepted("ERROR\nOK"));
    }
}

async fn l6_status(address: &str) -> Result<Status, String> {
    let mut fields = BTreeMap::new();
    let mut warnings = Vec::new();
    for (command, body) in [
        ("API_GetBaseInfo", ""),
        ("API_GetStorageInfo", ""),
        (
            "API_GeneralQuery",
            r#"{"keys":[{"key":"mic_switch"},{"key":"record_status"}]}"#,
        ),
    ] {
        match crate::adapters::l6_read(address, command, body, Duration::from_secs(8)).await {
            Ok(value) => {
                if command == "API_GetBaseInfo" {
                    if let Some(version) = value["version"].as_str() {
                        fields.insert("FWversion".into(), version.into());
                    }
                }
                if command == "API_GetStorageInfo" {
                    for (key, target) in [("sdcapacity", "TotalSize"), ("sdspare", "FreeSize")] {
                        if let Some(n) = value[key]
                            .as_u64()
                            .or_else(|| value[key].as_str().and_then(|s| s.parse().ok()))
                        {
                            fields.insert(target.into(), format!("{} KB", n));
                        }
                    }
                }
                for array in ["int_params", "string_params"] {
                    if let Some(items) = value[array].as_array() {
                        for item in items {
                            if let Some(key) = item["key"].as_str() {
                                let raw = item["value"]
                                    .as_str()
                                    .map(str::to_string)
                                    .unwrap_or_else(|| item["value"].to_string());
                                if key == "mic_switch" {
                                    fields.insert(
                                        "mic".into(),
                                        if raw == "on" || raw == "1" {
                                            "ON"
                                        } else {
                                            "OFF"
                                        }
                                        .into(),
                                    );
                                } else if key == "record_status" {
                                    fields.insert("Record".into(), raw);
                                }
                            }
                        }
                    }
                }
                fields.extend(crate::adapters::flat_fields(&value));
            }
            Err(_) => warnings.push(format!("{command} 设备未提供")),
        }
    }
    if fields.is_empty() {
        return Err("设备没有返回可识别状态".into());
    }
    Ok(Status { fields, warnings })
}
async fn l6_command(address: &str, command: &str, value: Option<&str>) -> Result<(), String> {
    let (api,body)=match command{
        "record"=>("API_RecordOpt",r#"{"switch":"on"}"#.to_string()),"stop"=>("API_RecordOpt",r#"{"switch":"off"}"#.to_string()),
        "capture"=>("API_AppSnap",String::new()),"event"=>("API_EmerRec",String::new()),"format"=>("API_MmcFormat",String::new()),
        "authTime"=>return Err("此记录仪不支持录音授权时长".into()),
        "mic"=>("API_GeneralSave",serde_json::json!({"int_params":[],"string_params":[{"key":"mic_switch","value":if value==Some("ON"){"on"}else{"off"}}]}).to_string()),
        _=>return Err("设备不支持此操作".into()),
    };
    let session = crate::adapters::l6_session(address).await?;
    let result = crate::adapters::l6_request(
        address,
        api,
        Some(&body),
        Some(&session),
        Duration::from_secs(25),
    )
    .await?;
    if result.get("code").is_some_and(|code| {
        code.as_i64()
            .or_else(|| code.as_str().and_then(|s| s.parse().ok()))
            .is_some_and(|code| code != 0)
    }) {
        return Err("设备未接受此操作".into());
    }
    Ok(())
}
