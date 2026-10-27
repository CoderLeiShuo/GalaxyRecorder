mod connection;
mod network;
mod protocol;

use connection::{client, endpoint, text, ConnectionState, Recorder};
use protocol::{authority, media_path, parse_listing_page, Video};
use reqwest::Url;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{Emitter, Manager, State};
use tokio::{io::AsyncWriteExt, sync::Mutex};

#[derive(Default)]
struct Transfers(Mutex<HashMap<String, Arc<AtomicBool>>>);

#[derive(Serialize)]
struct Listing {
    videos: Vec<Video>,
    warning: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    task_id: String,
    received: u64,
    total: Option<u64>,
}

#[tauri::command]
async fn probe_recorder(
    app: tauri::AppHandle,
    state: State<'_, ConnectionState>,
    address: String,
) -> Result<Recorder, String> {
    connection::connect(&app, &state, address).await
}

#[tauri::command]
async fn discover_recorder(
    app: tauri::AppHandle,
    state: State<'_, ConnectionState>,
    preferred: Option<String>,
    subnet: Option<String>,
) -> Result<Recorder, String> {
    connection::discover(&app, &state, preferred, subnet).await
}

#[tauri::command]
async fn get_recorder_network(app: tauri::AppHandle) -> Result<network::NetworkInfo, String> {
    network::inspect(&app).await
}

#[tauri::command]
async fn check_recorder_network(
    app: tauri::AppHandle,
    state: State<'_, ConnectionState>,
) -> Result<network::NetworkInfo, String> {
    let _operation = state.operation.lock().await;
    state.begin("network-check");
    let was_bound = network::inspect(&app)
        .await
        .map(|info| info.bound)
        .unwrap_or(false);
    let result = network::prepare(&app, false).await;
    if let Ok(info) = &result {
        state.network(info.clone());
        state.log(
            "network-check",
            format!("Wi-Fi 自检通过：绑定={}；本次未发送设备请求", info.bound),
        );
    }
    if !was_bound {
        let _ = network::release(&app).await;
    }
    state.finish(result.as_ref().err().map(String::as_str));
    result
}

#[tauri::command]
fn get_connection_diagnostics(
    state: State<'_, ConnectionState>,
) -> Result<connection::Diagnostics, String> {
    state.snapshot()
}

#[tauri::command]
fn cancel_recorder_discovery(state: State<'_, ConnectionState>) {
    state.cancel();
}

#[tauri::command]
async fn release_recorder_network(
    app: tauri::AppHandle,
    state: State<'_, ConnectionState>,
) -> Result<(), String> {
    state.cancel();
    let _operation = state.operation.lock().await;
    network::release(&app).await
}

#[tauri::command]
async fn list_videos(
    app: tauri::AppHandle,
    address: String,
    category: String,
) -> Result<Listing, String> {
    network::prepare(&app, false).await?;
    let property = match category.as_str() {
        "normal" => "DCIM",
        "event" => "EVENT",
        _ => return Err("不支持的录像分类".into()),
    };
    let http = client()?;
    let mut videos = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 0;
    let mut repeated = 0;
    for _ in 0..200 {
        let mut page = None;
        let mut failure = "录像列表读取失败".to_string();
        for count in [50, 20, 1] {
            let mut url = endpoint(&address, "dir", property)?;
            url.query_pairs_mut()
                .append_pair("format", "all")
                .append_pair("count", &count.to_string())
                .append_pair("from", &offset.to_string());
            match text(&http, url, 10)
                .await
                .and_then(|xml| parse_listing_page(&xml, &category))
            {
                Ok((items, entries)) => {
                    page = Some((items, entries, count));
                    break;
                }
                Err(error) => failure = error,
            }
        }
        let Some((items, entries, count)) = page else {
            return if videos.is_empty() {
                Err(failure)
            } else {
                Ok(Listing {
                    videos,
                    warning: Some(format!("列表尚未完整读取：{failure}")),
                })
            };
        };
        let before = videos.len();
        for video in items {
            if seen.insert(video.id.clone()) {
                videos.push(video);
            }
        }
        if entries == 0 || entries < count {
            return Ok(Listing {
                videos,
                warning: None,
            });
        }
        repeated = if videos.len() == before {
            repeated + 1
        } else {
            0
        };
        if repeated >= 3 {
            return Ok(Listing {
                videos,
                warning: Some("设备连续返回重复页，已保留读取到的录像，请刷新重试".into()),
            });
        }
        offset += entries;
    }
    Ok(Listing {
        videos,
        warning: Some("已达到单次读取上限，列表可能不完整".into()),
    })
}

fn media_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("media"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn saved_directory(app: tauri::AppHandle) -> Result<String, String> {
    let path = media_directory(&app)?;
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn cancel_download(task_id: String, state: State<'_, Transfers>) -> Result<(), String> {
    if let Some(cancel) = state.0.lock().await.get(&task_id) {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
async fn download_video(
    app: tauri::AppHandle,
    state: State<'_, Transfers>,
    address: String,
    video: Video,
    task_id: String,
) -> Result<String, String> {
    if task_id.is_empty()
        || task_id.len() > 64
        || !task_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("下载任务标识不正确".into());
    }
    network::prepare(&app, false).await?;
    let address = authority(&address)?;
    let path = media_path(&video.path)?;
    let name = path.rsplit('/').next().ok_or("缺少录像文件名")?;
    let mut url = Url::parse(&format!("http://{address}")).map_err(|e| e.to_string())?;
    url.set_path(&path);
    let dir = media_directory(&app)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = state.0.lock().await;
        if jobs.contains_key(&task_id) {
            return Err("任务已在下载".into());
        }
        jobs.insert(task_id.clone(), cancel.clone());
    }
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(180)
        .collect();
    let final_path = dir.join(format!("{task_id}_{safe_name}"));
    let partial = final_path.with_extension("part");
    let result=async {
        tokio::fs::create_dir_all(&dir).await.map_err(|e|e.to_string())?;
        let mut response=client()?.get(url).timeout(Duration::from_secs(1800)).header("Accept-Encoding","identity").send().await.map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
        let total=response.content_length();
        if response.headers().get("content-type").and_then(|h|h.to_str().ok()).is_some_and(|value|value.contains("text/")||value.contains("json")){return Err("设备返回了文本内容，未保存为录像".to_string());}
        let mut file=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&partial).await.map_err(|e|e.to_string())?;
        let mut received=0u64;let mut first=Vec::new();let mut last_event=std::time::Instant::now();
        loop {
            let next=tokio::select! {
                value=response.chunk()=>value.map_err(|e|e.to_string())?,
                _=async {while !cancel.load(Ordering::Relaxed){tokio::time::sleep(Duration::from_millis(100)).await;}}=>return Err("下载已取消".into()),
            };
            let Some(chunk)=next else{break;};
            if first.len()<512{first.extend_from_slice(&chunk[..chunk.len().min(512-first.len())]);}
            received=received.checked_add(chunk.len() as u64).ok_or("录像文件过大")?;
            file.write_all(&chunk).await.map_err(|e|e.to_string())?;
            if last_event.elapsed()>Duration::from_millis(150){let _=app.emit("download-progress",Progress{task_id:task_id.clone(),received,total});last_event=std::time::Instant::now();}
        }
        if cancel.load(Ordering::Relaxed){return Err("下载已取消".into());}
        if received==0||total.is_some_and(|length|length!=received){return Err("录像下载不完整，请重试".into());}
        // Reject error pages even when the device labels them as binary data.
        let first_text=String::from_utf8_lossy(&first).trim_start().to_lowercase();
        if first_text.starts_with("<html")||first_text.starts_with("<!doctype")||first_text.starts_with("<?xml"){return Err("下载响应不是录像，未发布文件".into());}
        file.flush().await.map_err(|e|e.to_string())?;file.sync_all().await.map_err(|e|e.to_string())?;drop(file);
        tokio::fs::rename(&partial,&final_path).await.map_err(|e|e.to_string())?;
        let saved_path=final_path.to_string_lossy().to_string();let mut saved=video;saved.size=received;saved.saved_path=Some(saved_path.clone());
        let metadata=final_path.with_extension("json");let metadata_partial=final_path.with_extension("json.part");
        tokio::fs::write(&metadata_partial,serde_json::to_vec(&saved).map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?;
        tokio::fs::rename(metadata_partial,metadata).await.map_err(|e|e.to_string())?;
        let _=app.emit("download-progress",Progress{task_id:task_id.clone(),received,total:Some(received)});
        Ok(saved_path)
    }.await;
    state.0.lock().await.remove(&task_id);
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
        let _ = tokio::fs::remove_file(&final_path).await;
        let _ = tokio::fs::remove_file(final_path.with_extension("json.part")).await;
    }
    result
}

#[tauri::command]
async fn list_saved(app: tauri::AppHandle) -> Result<Vec<Video>, String> {
    let directory = media_directory(&app)?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(&directory)
        .await
        .map_err(|e| e.to_string())?;
    let mut videos = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        if entry.path().extension().is_some_and(|ext| ext == "json") {
            let bytes = tokio::fs::read(entry.path())
                .await
                .map_err(|e| e.to_string())?;
            if let Ok(video) = serde_json::from_slice::<Video>(&bytes) {
                if video.saved_path.as_ref().is_some_and(|path| {
                    let p = PathBuf::from(path);
                    p.parent() == Some(directory.as_path()) && p.is_file()
                }) {
                    videos.push(video);
                }
            }
        }
    }
    Ok(videos)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(network::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Transfers::default())
        .manage(ConnectionState::default())
        .invoke_handler(tauri::generate_handler![
            probe_recorder,
            discover_recorder,
            get_recorder_network,
            check_recorder_network,
            get_connection_diagnostics,
            cancel_recorder_discovery,
            release_recorder_network,
            list_videos,
            download_video,
            cancel_download,
            list_saved,
            saved_directory
        ])
        .run(tauri::generate_context!())
        .expect("银河记录仪启动失败");
}
