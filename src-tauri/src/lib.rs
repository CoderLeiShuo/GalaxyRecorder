mod adapters;
mod connection;
mod device;
mod editing;
mod journal;
mod library;
mod listing;
mod media;
mod metadata;
mod mobile;
mod network;
mod protocol;
mod resume;
mod settings;
mod transfers;
mod vehicle;

use connection::{client, endpoint, response_text, ConnectionState, Recorder};
use listing::{Listing, PageError};
use protocol::{authority, video_path, Video};
use reqwest::Url;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex;

#[derive(Default)]
pub(crate) struct Transfers(Mutex<HashMap<String, Arc<AtomicBool>>>);

#[derive(Default)]
struct ListingState {
    operation: Mutex<()>,
    page_sizes: std::sync::Mutex<HashMap<(String, String), usize>>,
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
    profile: Option<String>,
) -> Result<Recorder, String> {
    state.set_profile(profile.as_deref())?;
    connection::connect(&app, &state, address).await
}

#[tauri::command]
async fn discover_recorder(
    app: tauri::AppHandle,
    state: State<'_, ConnectionState>,
    preferred: Option<String>,
    subnet: Option<String>,
    profile: Option<String>,
) -> Result<Recorder, String> {
    state.set_profile(profile.as_deref())?;
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
    let transfers = transfers::connection_lost(&app).await;
    let _operation = state.operation.lock().await;
    network::release(&app).await?;
    #[cfg(target_os = "android")]
    let _ = mobile::call(&app, "releaseWifi", serde_json::json!({})).await;
    transfers
}

#[tauri::command]
async fn list_videos(
    app: tauri::AppHandle,
    diagnostics: State<'_, ConnectionState>,
    listings: State<'_, ListingState>,
    address: String,
    category: String,
) -> Result<Listing, String> {
    // Each adapter serializes listing requests; A7 cannot serve parallel categories.
    let _operation = listings.operation.lock().await;
    let address = authority(&address)?;
    let family = diagnostics.family(&address);
    if family != "A7" {
        network::prepare(&app, false).await?;
        return adapters::list(&address, &category, &family).await;
    }
    let property = match category.as_str() {
        "normal" => "DCIM",
        "event" => "EVENT",
        _ => return Err("不支持的录像分类".into()),
    };
    diagnostics.log(
        "listing",
        format!("开始读取 {category}；设备={address}，property={property}"),
    );
    let result = async {
        network::prepare(&app, false).await?;
        let http = client()?;
        let key = (address.clone(), category.clone());
        let preferred = listings
            .page_sizes
            .lock()
            .ok()
            .and_then(|sizes| sizes.get(&key).copied());
        let result = listing::read(
            &category,
            preferred,
            |offset, count| {
                let http = &http;
                let address = &address;
                async move {
                    let mut url = endpoint(address, "dir", property).map_err(PageError::Other)?;
                    url.query_pairs_mut()
                        .append_pair("format", "all")
                        .append_pair("count", &count.to_string())
                        .append_pair("from", &offset.to_string());
                    let response = http
                        .get(url)
                        .header("Accept-Encoding", "identity")
                        .timeout(Duration::from_secs(12))
                        .send()
                        .await
                        .map_err(|error| PageError::Other(format!("设备未响应：{error}")))?;
                    if response.status() != reqwest::StatusCode::OK {
                        return Err(PageError::Http(response.status().as_u16()));
                    }
                    response_text(response, 16 * 1024 * 1024)
                        .await
                        .map_err(PageError::Other)
                }
            },
            |message| diagnostics.log("listing", format!("{category}: {message}")),
        )
        .await?;
        if let Some(size) = result.page_size {
            if let Ok(mut sizes) = listings.page_sizes.lock() {
                sizes.insert(key, size);
            }
        }
        Ok::<_, String>(result)
    }
    .await;
    match &result {
        Ok(listing) => diagnostics.log(
            "listing",
            format!(
                "{category} 读取结束：{} 条录像；{}",
                listing.videos.len(),
                listing.warning.as_deref().unwrap_or("列表完整")
            ),
        ),
        Err(error) => diagnostics.log("listing-error", format!("{category}: {error}")),
    }
    result
}

pub(crate) fn media_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
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
) -> Result<Video, String> {
    let mask = settings::read(&app).await?.mask_downloads;
    download_file(&app, &state, address, video, task_id, mask).await
}

pub(crate) async fn download_file(
    app: &tauri::AppHandle,
    state: &Transfers,
    address: String,
    video: Video,
    task_id: String,
    mask_vin: bool,
) -> Result<Video, String> {
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
    let path = video_path(&video)?;
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
    // Recover a fully received source if the process stopped during local masking/saving.
    if !partial.exists() && final_path.is_file() && !final_path.with_extension("json").is_file() {
        let _ = tokio::fs::rename(&final_path, &partial).await;
    }
    let result = async {
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| e.to_string())?;
        let mut last_event = std::time::Instant::now() - Duration::from_secs(1);
        let received = resume::download(
            &client()?,
            url.as_str(),
            &partial,
            cancel.clone(),
            |received, total| {
                if last_event.elapsed() > Duration::from_millis(150) {
                    let _ = app.emit(
                        "download-progress",
                        Progress {
                            task_id: task_id.clone(),
                            received,
                            total,
                        },
                    );
                    transfers::progress(app, &task_id, received, total);
                    last_event = std::time::Instant::now();
                }
            },
        )
        .await?;
        let mut file = tokio::fs::File::open(&partial)
            .await
            .map_err(|e| e.to_string())?;
        let mut first = vec![0; received.min(512) as usize];
        tokio::io::AsyncReadExt::read_exact(&mut file, &mut first)
            .await
            .map_err(|e| e.to_string())?;
        drop(file);
        let text = String::from_utf8_lossy(&first).trim_start().to_lowercase();
        if text.starts_with("<html") || text.starts_with("<!doctype") || text.starts_with("<?xml") {
            let _ = tokio::fs::remove_file(&partial).await;
            resume::remove_checkpoint(&partial).await;
            return Err("下载响应不是录像，未发布文件".into());
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("下载已暂停，进度已保留".into());
        }
        tokio::fs::rename(&partial, &final_path)
            .await
            .map_err(|e| e.to_string())?;
        let mut saved = video;
        saved.size = received;
        saved.saved_path = Some(final_path.to_string_lossy().to_string());
        saved.storage = None;
        transfers::progress(app, &task_id, received, Some(received));
        if mask_vin {
            transfers::phase(app, &task_id, "masking");
            saved = editing::mask_download(app, saved, cancel.clone()).await?;
        }
        transfers::phase(app, &task_id, "saving");
        let saved = library::store(app, saved, true).await?;
        if mask_vin {
            let _ = tokio::fs::remove_file(&final_path).await;
        }
        resume::remove_checkpoint(&partial).await;
        Ok(saved)
    }
    .await;
    state.0.lock().await.remove(&task_id);
    if result.is_err() {
        // A completed source can also be resumed after interrupted local processing.
        if final_path.is_file() {
            let _ = tokio::fs::rename(&final_path, &partial).await;
        }
        let _ = tokio::fs::remove_file(final_path.with_extension("json.part")).await;
        let _ = tokio::fs::remove_file(final_path.with_extension("masked.part.mp4")).await;
        let _ = tokio::fs::remove_file(final_path.with_extension("masked.mp4")).await;
    }
    result
}

#[tauri::command]
async fn list_saved(app: tauri::AppHandle) -> Result<Vec<Video>, String> {
    library::list(&app).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            journal::initialize(app.handle());
            Ok(())
        })
        .plugin(network::init())
        .plugin(mobile::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(media::MediaServer::default())
        .manage(editing::Exports::default())
        .manage(std::sync::Arc::new(transfers::Queue::default()))
        .plugin(tauri_plugin_opener::init())
        .manage(Transfers::default())
        .manage(ListingState::default())
        .manage(ConnectionState::default())
        .invoke_handler(tauri::generate_handler![
            metadata::video_metadata,
            journal::get_operation_logs,
            journal::export_operation_logs,
            journal::record_client_event,
            library::share_saved_many,
            transfers::control_transfers,
            mobile::native_media,
            settings::get_settings,
            settings::rename_app,
            settings::save_appearance,
            settings::set_connection_profile,
            settings::confirm_vehicle,
            settings::save_settings,
            settings::choose_download_directory,
            device::recorder_status,
            device::recorder_command,
            device::open_media,
            device::live_media,
            library::export_saved,
            library::delete_saved,
            library::rename_saved,
            library::share_saved,
            editing::export_video,
            editing::cancel_export,
            transfers::enqueue_transfers,
            transfers::list_transfers,
            transfers::cancel_transfer,
            transfers::delete_transfer,
            transfers::retry_transfer,
            probe_recorder,
            connection::check_recorder_connection,
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
        .expect("吉行影记启动失败");
}
