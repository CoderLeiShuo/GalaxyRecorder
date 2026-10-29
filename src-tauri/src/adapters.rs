//! Locally implemented adapters, based on the original APK's protocol evidence.
use crate::{
    connection::{client, endpoint, response_text},
    protocol::authority,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

pub const CHOICES: [&str; 7] = ["AUTO", "A7", "E5", "L6", "GENERIC1", "GENERIC2", "GENERIC3"];
pub fn profile(input: Option<&str>) -> Result<&'static str, String> {
    let input = input.unwrap_or("AUTO");
    if input == "X7" {
        return Ok("E5");
    }
    CHOICES
        .into_iter()
        .find(|p| *p == input)
        .ok_or_else(|| "不支持的连接方案".into())
}
pub fn family(input: &str) -> &str {
    match input {
        "GENERIC1" | "X7" => "E5",
        "GENERIC2" => "A7",
        "GENERIC3" => "L6",
        p => p,
    }
}
pub const FACTORY_ADDRESSES: [&str; 6] = [
    "192.168.173.2",
    "193.168.0.1",
    "192.168.43.1",
    "192.168.42.1",
    "192.72.1.1",
    "192.168.1.254",
];
pub fn factory_ip(ip: std::net::Ipv4Addr) -> bool {
    matches!(ip.octets(), [193, 168, 0, 1] | [192, 72, 1, 1])
}
#[derive(Debug, Clone)]
pub struct Detection {
    pub family: String,
    pub fields: BTreeMap<String, String>,
}
pub fn l6_payload(text: &str) -> Result<Value, String> {
    if text.len() > 16 * 1024 * 1024 {
        return Err("设备响应过大".into());
    }
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| "不是 Vyou 设备响应")?;
    let code = root
        .get("errcode")
        .and_then(|c| c.as_i64().or_else(|| c.as_str()?.parse().ok()))
        .ok_or("设备响应缺少状态码")?;
    if code != 0 {
        return Err(format!("设备返回状态码 {code}"));
    }
    match root.get("data") {
        Some(Value::Object(_)) => Ok(root["data"].clone()),
        Some(Value::String(data)) if !data.trim().is_empty() => {
            let data: Value = serde_json::from_str(data).map_err(|_| "设备 data 格式不正确")?;
            if !data.is_object() {
                return Err("设备 data 格式不正确".into());
            }
            Ok(data)
        }
        None | Some(Value::Null) | Some(Value::String(_)) => Ok(root),
        _ => Err("设备 data 格式不正确".into()),
    }
}
pub fn is_l6(value: &Value) -> bool {
    value["model"]
        .as_str()
        .is_some_and(|m| !m.trim().is_empty())
        && ["mfr", "macaddr", "uuid"]
            .iter()
            .any(|k| value[*k].as_str().is_some_and(|v| !v.trim().is_empty()))
}
pub fn flat_fields(value: &Value) -> BTreeMap<String, String> {
    fn visit(value: &Value, prefix: &str, result: &mut BTreeMap<String, String>, depth: usize) {
        if depth > 4 {
            return;
        }
        if let Some(object) = value.as_object() {
            for (key, value) in object {
                let key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                if value.is_object() {
                    visit(value, &key, result, depth + 1)
                } else if let Some(v) = value.as_str() {
                    result.insert(key, v.into());
                } else if value.is_number() {
                    result.insert(key, value.to_string());
                }
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(value, "", &mut result, 0);
    result
}
pub async fn l6_request(
    address: &str,
    command: &str,
    body: Option<&str>,
    session: Option<&str>,
    timeout: Duration,
) -> Result<Value, String> {
    if !command.starts_with("API_") && !command.starts_with("APP_") {
        return Err("不支持的设备命令".into());
    }
    if !command
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("设备命令格式错误".into());
    }
    let url = format!("http://{}/vcam/cmd.cgi?cmd={command}", authority(address)?);
    let http = client()?;
    let request = if let Some(body) = body {
        http.post(url)
            .header("Content-Type", "application/json;charset=UTF-8")
            .body(body.to_string())
    } else {
        http.get(url)
    };
    let session = session.unwrap_or("");
    let response = request
        .header("sessionid", session)
        .header("Cookie", format!("SessionID={session}"))
        .header("Connection", "close")
        .timeout(timeout)
        .send()
        .await
        .map_err(|_| "设备请求未完成")?
        .error_for_status()
        .map_err(|_| "设备未接受请求")?;
    l6_payload(&response_text(response, 16 * 1024 * 1024).await?)
}
pub fn accepted_fields(body: &str) -> bool {
    !body.contains(['<', '>'])
        && !body.lines().any(|line| {
            let line = line.trim().to_ascii_lowercase();
            line == "error"
                || line.starts_with("error ")
                || line.starts_with("unsupported")
                || line.parse::<i64>().is_ok_and(|n| n != 0)
        })
}
async fn config_family(address: &str, body: &str, timeout: Duration) -> String {
    let fields = crate::device::fields(body);
    if fields
        .values()
        .any(|value| value.to_ascii_lowercase().contains("mstar"))
    {
        return "E5".into();
    }
    // Both families can answer DVR=OK. Look at actual, read-only directory paths.
    for (property, family) in [("DCIM", "A7"), ("MOVIE", "E5")] {
        let result = async {
            let mut url = endpoint(address, "dir", property)?;
            url.query_pairs_mut()
                .append_pair("format", "all")
                .append_pair("count", "1")
                .append_pair("from", "0");
            let response = client()?
                .get(url)
                .timeout(timeout)
                .send()
                .await
                .map_err(|_| "设备未响应")?
                .error_for_status()
                .map_err(|_| "无此目录")?;
            let body = response_text(response, 1024 * 1024).await?;
            crate::protocol::parse_listing_for(&body, "normal", family)
        }
        .await;
        if result.is_ok_and(|page| !page.videos.is_empty()) {
            return family.into();
        }
    }
    "A7".into()
}
pub async fn probe(address: &str, selected: &str, timeout: Duration) -> Result<Detection, String> {
    let selected = profile(Some(selected))?;
    let mode = family(selected);
    let http = client()?;
    if mode == "A7" || mode == "AUTO" {
        let response = http
            .get(endpoint(address, "get", "DVR")?)
            .timeout(timeout)
            .send()
            .await;
        if let Ok(response) = response {
            if response.status().is_success() {
                let body = response_text(response, 65536).await?;
                if crate::protocol::a7_probe_accepted(&body) {
                    let detected = if mode == "AUTO" {
                        config_family(address, &body, timeout).await
                    } else {
                        "A7".into()
                    };
                    return Ok(Detection {
                        family: detected,
                        fields: crate::device::fields(&body),
                    });
                }
            }
        }
        if mode == "A7" {
            if let Ok(response) = http
                .get(endpoint(address, "get", "Camera.Menu.FWversion")?)
                .timeout(timeout)
                .send()
                .await
            {
                if response.status().is_success() {
                    let body = response_text(response, 65536).await?;
                    if crate::protocol::a7_probe_accepted(&body) {
                        return Ok(Detection {
                            family: "A7".into(),
                            fields: crate::device::fields(&body),
                        });
                    }
                }
            }
            return Err("未识别到 A7 记录仪".into());
        }
    }
    if mode == "E5" || mode == "AUTO" {
        let response = http
            .get(endpoint(address, "get", "Camera.Menu.FWversion")?)
            .timeout(timeout)
            .send()
            .await;
        if let Ok(response) = response {
            if response.status().is_success() {
                let body = response_text(response, 65536).await?;
                let fields = crate::device::fields(&body);
                if accepted_fields(&body)
                    && fields.iter().any(|(k, v)| {
                        k.to_ascii_lowercase().ends_with("fwversion") && !v.is_empty()
                    })
                {
                    return Ok(Detection {
                        family: "E5".into(),
                        fields,
                    });
                }
            }
        }
        if mode == "E5" {
            return Err("未识别到 E5 记录仪".into());
        }
    }
    if mode == "L6" || mode == "AUTO" {
        for body in [None, Some("")] {
            if let Ok(value) = l6_request(address, "API_GetBaseInfo", body, None, timeout).await {
                if is_l6(&value) {
                    return Ok(Detection {
                        family: "L6".into(),
                        fields: flat_fields(&value),
                    });
                }
            }
        }
        // The original L6 adapter falls back to NPD/Config.cgi on its factory IP.
        if address.split(':').next() == Some("192.72.1.1") {
            return Box::pin(probe(address, "E5", timeout)).await;
        }
    }
    Err("设备未返回所选方案的识别信息".into())
}
static L6_SESSIONS: std::sync::OnceLock<
    tokio::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, String)>>,
> = std::sync::OnceLock::new();
fn sessions(
) -> &'static tokio::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, String)>> {
    L6_SESSIONS.get_or_init(Default::default)
}
pub async fn reset_session(address: &str) {
    sessions().lock().await.remove(address);
}
pub async fn l6_session(address: &str) -> Result<String, String> {
    let mut sessions = sessions().lock().await;
    if let Some((time, session)) = sessions.get(address) {
        if time.elapsed() < Duration::from_secs(120) {
            return Ok(session.clone());
        }
    }
    let value = l6_request(
        address,
        "API_RequestSessionID",
        Some(""),
        None,
        Duration::from_secs(10),
    )
    .await?;
    let session = value["acSessionId"]
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 512
                && s.bytes()
                    .all(|b| b > 32 && b < 127 && b != b';' && b != b',')
        })
        .ok_or("记录仪会话格式错误")?
        .to_string();
    // An app-session UID. Used only for authentication to the local recorder.
    static UID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let uid = UID.get_or_init(|| uuid::Uuid::new_v4().to_string());
    let body = json!({"user":"admin","password":"admin","level":0,"uid":uid}).to_string();
    l6_request(
        address,
        "API_RequestCertificate",
        Some(&body),
        Some(&session),
        Duration::from_secs(10),
    )
    .await?;
    sessions.insert(
        address.to_string(),
        (std::time::Instant::now(), session.clone()),
    );
    Ok(session)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_original_visible_profiles_and_legacy_alias_are_mapped() {
        assert_eq!(family(profile(Some("GENERIC1")).unwrap()), "E5");
        assert_eq!(family(profile(Some("GENERIC2")).unwrap()), "A7");
        assert_eq!(family(profile(Some("GENERIC3")).unwrap()), "L6");
        assert_eq!(profile(Some("X7")).unwrap(), "E5");
        assert!(profile(Some("unknown")).is_err());
    }
    #[test]
    fn l6_requires_success_and_device_identity() {
        assert!(l6_payload(r#"{"errcode":4,"data":{}}"#).is_err());
        assert!(!is_l6(&json!({"model":"car"})));
        assert!(is_l6(
            &l6_payload(r#"{"errcode":0,"data":"{\"model\":\"Vyou\",\"mfr\":\"Geely\"}"}"#)
                .unwrap()
        ));
    }
    #[test]
    fn device_data_is_flattened_without_inferring_year_from_firmware() {
        let v = flat_fields(
            &json!({"car":{"VehicleModel":"星越L","ModelYear":2026},"version":"2025.1"}),
        );
        assert_eq!(crate::vehicle::info(&v).unwrap().name, "星越L2026款");
    }
    #[test]
    fn only_two_explicit_factory_addresses_extend_the_lan_rule() {
        assert!(factory_ip("193.168.0.1".parse().unwrap()));
        assert!(factory_ip("192.72.1.1".parse().unwrap()));
        assert!(!factory_ip("193.168.0.2".parse().unwrap()));
    }
}

pub async fn l6_read(
    address: &str,
    command: &str,
    body: &str,
    timeout: Duration,
) -> Result<Value, String> {
    let mut error = String::new();
    for _ in 0..2 {
        let session = l6_session(address).await?;
        match l6_request(address, command, Some(body), Some(&session), timeout).await {
            Ok(value) => return Ok(value),
            Err(e) => {
                error = e;
                reset_session(address).await;
            }
        }
    }
    Err(error)
}
pub async fn list(
    address: &str,
    category: &str,
    family: &str,
) -> Result<crate::listing::Listing, String> {
    if !["normal", "event"].contains(&category) {
        return Err("不支持的录像分类".into());
    }
    if family == "L6" {
        let command = if category == "normal" {
            "APP_PlaybackListReq"
        } else {
            "APP_EventListReq"
        };
        let payload = l6_read(address, command, "", Duration::from_secs(15)).await?;
        return l6_listing(&payload, category);
    }
    let properties: Vec<&str> = if category == "normal" {
        vec!["MOVIE"]
    } else {
        vec!["SOS", "LOCK"]
    };
    let mut videos = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut warnings = Vec::new();
    let mut readable = false;
    for property in properties {
        let mut offset = 0usize;
        loop {
            if offset >= 200000 {
                return Err("录像列表超过上限".into());
            }
            let mut url = endpoint(address, "dir", property)?;
            url.query_pairs_mut()
                .append_pair("format", "all")
                .append_pair("count", "32")
                .append_pair("from", &offset.to_string());
            let page = async {
                let response = client()?
                    .get(url)
                    .timeout(Duration::from_secs(12))
                    .send()
                    .await
                    .map_err(|_| "设备录像列表未响应")?
                    .error_for_status()
                    .map_err(|_| "设备不支持此录像分类")?;
                let text = response_text(response, 16 * 1024 * 1024).await?;
                crate::protocol::parse_listing_for(&text, category, "E5")
            }
            .await;
            match page {
                Ok(page) => {
                    readable = true;
                    if page.entries == 0 {
                        break;
                    }
                    if page.videos.is_empty() {
                        return Err("设备返回了无法解析的录像条目".into());
                    }
                    let before = videos.len();
                    for video in page.videos {
                        if seen.insert(video.id.clone()) {
                            videos.push(video)
                        }
                    }
                    if before == videos.len() {
                        warnings.push("设备未返回下一页，保留已读录像".into());
                        break;
                    }
                    offset += page.entries;
                }
                Err(error) => {
                    if property != "LOCK" {
                        if !videos.is_empty() {
                            warnings.push(error);
                            break;
                        } else {
                            return Err(error);
                        }
                    }
                    break;
                }
            }
        }
    }
    if !readable {
        return Err("设备没有返回录像列表".into());
    }
    Ok(crate::listing::Listing {
        videos,
        warning: (!warnings.is_empty()).then(|| warnings.join("；")),
        page_size: Some(32),
    })
}
fn epoch_date(seconds: u64) -> Result<(String, String), String> {
    if seconds > 4_102_444_800 {
        return Err("录像时间超出范围".into());
    }
    let days = (seconds / 86400) as i64 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    Ok((
        format!("{year:04}-{month:02}-{day:02}"),
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600 % 24,
            seconds / 60 % 60,
            seconds % 60
        ),
    ))
}
fn number(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}
pub fn l6_listing(value: &Value, category: &str) -> Result<crate::listing::Listing, String> {
    let key = if category == "normal" {
        "file"
    } else {
        "event"
    };
    let Some(items) = value[key].as_array() else {
        if category == "event" && value.get(key).is_none() || value["num"].as_u64() == Some(0) {
            return Ok(crate::listing::Listing {
                videos: vec![],
                warning: None,
                page_size: None,
            });
        }
        return Err("设备没有返回录像数组".into());
    };
    if items.len() > 50000 {
        return Err("录像列表超过上限".into());
    }
    if items.is_empty() && number(&value["num"]).is_some_and(|n| n > 0) {
        return Err("录像列表条数与内容不一致".into());
    }
    let mut videos = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let (name, start, end) = if category == "normal" {
            (
                item["name"].as_str().ok_or("录像缺少文件名")?,
                number(&item["starttime"]),
                number(&item["endtime"]),
            )
        } else if let Some(name) = item["bvideoname"].as_str().filter(|s| !s.is_empty()) {
            (name, number(&item["bstarttime"]), number(&item["bendtime"]))
        } else if let Some(name) = item["svideoname"].as_str().filter(|s| !s.is_empty()) {
            (name, number(&item["sstarttime"]), number(&item["sendtime"]))
        } else if item["imgname"].as_str().is_some_and(|s| !s.is_empty()) {
            continue;
        } else {
            return Err("紧急录像缺少文件名".into());
        };
        let path = crate::protocol::adapter_path(name, category, "L6")?;
        let start = start.ok_or("录像缺少开始时间")?;
        let end = end.filter(|end| *end > start).ok_or("录像时间范围无效")?;
        let (date, time) = epoch_date(start)?;
        if seen.insert(path.clone()) {
            videos.push(crate::protocol::Video {
                id: format!("L6:{category}:{path}"),
                name: name.into(),
                path,
                category: category.into(),
                date,
                time,
                size: 0,
                duration: Some((end - start) as f64),
                storage: None,
                adapter: Some("L6".into()),
                thumbnail: None,
                exported_path: None,
                export_error: None,
                saved_path: None,
            });
        }
    }
    Ok(crate::listing::Listing {
        videos,
        warning: None,
        page_size: None,
    })
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn fixture(
        kind: &'static str,
    ) -> (
        String,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        crate::protocol::register_fixture(&address);
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = requests.clone();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let log = log.clone();
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    loop {
                        let mut buf = [0; 4096];
                        let n = stream.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&data);
                        if let Some(end) = text.find("\r\n\r\n") {
                            let length = text[..end]
                                .lines()
                                .find_map(|line| {
                                    line.to_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|n| n.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if data.len() >= end + 4 + length {
                                break;
                            }
                        }
                    }
                    let request = String::from_utf8_lossy(&data).to_string();
                    log.lock().unwrap().push(request.clone());
                    let (status, body) = if kind == "E5Compat" && request.contains("property=DVR") {
                        (200, "OK")
                    } else if kind == "E5Compat" && request.contains("property=MOVIE") {
                        (
                            200,
                            "<list><file><name>/SD/MOVIE/v.mp4</name><size>2</size></file></list>",
                        )
                    } else if kind == "A7" && request.contains("property=DVR") {
                        (200, "CarModel=星越L\nModelYear=2026\nOK\n")
                    } else if kind == "E5" && request.contains("property=Camera.Menu.FWversion") {
                        (200, "Camera.Menu.FWversion=Mstar1\n")
                    } else if kind.starts_with("L6")
                        && request.contains("cmd=API_GetBaseInfo")
                        && (kind != "L6Post" || request.starts_with("POST"))
                    {
                        (
                            200,
                            r#"{"errcode":0,"data":{"model":"Vyou","mfr":"Geely","VehicleModel":"银河L6"}}"#,
                        )
                    } else if kind.starts_with("L6") && request.contains("cmd=API_RequestSessionID")
                    {
                        (
                            200,
                            r#"{"errcode":0,"data":{"acSessionId":"fixture-session"}}"#,
                        )
                    } else if kind.starts_with("L6")
                        && request.contains("cmd=API_RequestCertificate")
                    {
                        assert!(request.contains("admin"));
                        assert!(request
                            .to_lowercase()
                            .contains("sessionid: fixture-session"));
                        (200, r#"{"errcode":0,"data":{}}"#)
                    } else {
                        (404, "not supported")
                    };
                    let response=format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (address, requests, server)
    }
    #[tokio::test]
    async fn actual_http_auto_detects_all_families_and_l6_get_post_fallback() {
        for kind in ["A7", "E5", "E5Compat", "L6", "L6Post"] {
            let (address, log, server) = fixture(kind).await;
            let detected = probe(&address, "AUTO", Duration::from_secs(2))
                .await
                .unwrap_or_else(|e| {
                    panic!("fixture {kind}: {e}; requests={:?}", log.lock().unwrap())
                });
            assert_eq!(
                detected.family,
                if kind.starts_with("L6") {
                    "L6"
                } else if kind.starts_with("E5") {
                    "E5"
                } else {
                    kind
                }
            );
            if kind == "A7" {
                assert_eq!(
                    crate::vehicle::info(&detected.fields).unwrap().name,
                    "星越L2026款"
                );
            }
            if kind.starts_with("L6") {
                let token = l6_session(&address).await.unwrap();
                assert_eq!(token, "fixture-session");
                l6_session(&address).await.unwrap();
                assert_eq!(
                    log.lock()
                        .unwrap()
                        .iter()
                        .filter(|r| r.contains("cmd=API_RequestSessionID"))
                        .count(),
                    1
                );
            }
            server.abort();
        }
    }
    #[test]
    fn e5_listing_uses_sd_directories_and_rejects_cross_category_paths() {
        let page = crate::protocol::parse_listing_for(
            "<list><file><name>/MOVIE/20261028_080000.mp4</name><size>42</size></file></list>",
            "normal",
            "E5",
        )
        .unwrap();
        assert_eq!(page.videos[0].path, "/SD/MOVIE/20261028_080000.mp4");
        assert!(crate::protocol::adapter_path("SOS/v.mp4", "normal", "E5").is_err());
        assert!(crate::protocol::adapter_path("../v.mp4", "normal", "E5").is_err());
    }
    #[test]
    fn l6_normal_event_and_empty_lists_have_correct_metadata_and_safe_paths() {
        let normal = l6_listing(
            &json!({"file":[{"name":"v.mp4","starttime":0,"endtime":12,"size":999999}]}),
            "normal",
        )
        .unwrap();
        assert_eq!(normal.videos[0].date, "1970-01-01");
        assert_eq!(normal.videos[0].duration, Some(12.));
        assert_eq!(normal.videos[0].size, 0);
        let event = l6_listing(
            &json!({"event":[{"bvideoname":"e.mp4","bstarttime":100,"bendtime":112}]}),
            "event",
        )
        .unwrap();
        assert_eq!(event.videos[0].path, "/e.mp4");
        assert!(l6_listing(&json!({"num":5,"file":[]}), "normal").is_err());
        assert!(l6_listing(
            &json!({"file":[{"name":"../v.mp4","starttime":0,"endtime":12}]}),
            "normal"
        )
        .is_err());
        assert!(l6_listing(&json!({"num":0}), "normal")
            .unwrap()
            .videos
            .is_empty());
    }
}
