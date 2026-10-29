use crate::protocol::Video;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub address: String,
    pub video: Video,
    pub status: String,
    pub received: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    pub saved_path: Option<String>,
    #[serde(default)]
    pub mask_vin: bool,
    #[serde(default)]
    pub phase: Option<String>,
}
#[derive(Default)]
pub struct Queue {
    tasks: Mutex<Vec<Task>>,
    current: Mutex<Option<String>>,
    running: AtomicBool,
    loaded: tokio::sync::Mutex<bool>,
    storage: tokio::sync::Mutex<()>,
    changes: tokio::sync::Mutex<()>,
}
impl Queue {
    async fn load<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String> {
        let mut loaded = self.loaded.lock().await;
        if *loaded {
            return Ok(());
        }
        let file = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("transfers.json");
        if let Ok(bytes) = tokio::fs::read(file).await {
            let mut tasks: Vec<Task> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            for task in &mut tasks {
                if task.status == "waiting" || task.status == "downloading" {
                    task.status = "paused".into();
                    task.error = Some("上次传输已暂停，连接记录仪后可继续下载".into());
                }
            }
            let root = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("media");
            for task in tasks.iter_mut().filter(|t| t.status != "done") {
                if let Ok(path) = partial_path(&root, task) {
                    task.received = tokio::fs::metadata(path)
                        .await
                        .map(|m| m.len())
                        .unwrap_or(0);
                }
            }
            *self.tasks.lock().unwrap() = tasks;
        }
        *loaded = true;
        Ok(())
    }
    fn snapshot(&self) -> Vec<Task> {
        self.tasks.lock().unwrap().clone()
    }
    async fn publish<R: Runtime>(&self, app: &AppHandle<R>) {
        let _guard = self.storage.lock().await;
        let snapshot = self.snapshot();
        let _ = app.emit("transfer-state", &snapshot);
        if let Ok(root) = app.path().app_data_dir() {
            let _ = tokio::fs::create_dir_all(&root).await;
            let temp = root.join("transfers.json.part");
            if let Ok(bytes) = serde_json::to_vec(&snapshot) {
                if tokio::fs::write(&temp, bytes).await.is_ok() {
                    let _ = tokio::fs::rename(temp, root.join("transfers.json")).await;
                }
            }
        }
    }
}
fn kick(app: AppHandle, queue: Arc<Queue>) {
    if queue
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    tauri::async_runtime::spawn(async move {
        #[cfg(target_os = "android")]
        let service_key = format!("download:{}", uuid::Uuid::new_v4());
        loop {
            let next = {
                let mut tasks = queue.tasks.lock().unwrap();
                tasks
                    .iter_mut()
                    .find(|t| t.status == "waiting")
                    .map(|task| {
                        task.status = "downloading".into();
                        task.error = None;
                        task.phase = None;
                        *queue.current.lock().unwrap() = Some(task.id.clone());
                        task.clone()
                    })
            };
            let Some(task) = next else {
                queue.running.store(false, Ordering::SeqCst);
                // Close the race with enqueue between looking for work and releasing the runner.
                let pending = queue
                    .tasks
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|t| t.status == "waiting");
                if pending
                    && queue
                        .running
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                {
                    continue;
                }
                break;
            };
            queue.publish(&app).await;
            crate::journal::record("transfer", format!("开始下载，打码={}", task.mask_vin));
            #[cfg(target_os = "android")]
            let _ = crate::mobile::call(
                &app,
                "transfer",
                serde_json::json!({"active":true,"key":service_key,"message":format!("正在下载 {}",task.video.name)}),
            )
            .await;
            let result = {
                let state = app.state::<crate::Transfers>();
                crate::download_file(
                    &app,
                    &state,
                    task.address.clone(),
                    task.video.clone(),
                    task.id.clone(),
                    task.mask_vin,
                )
                .await
            };
            if result.is_err() {
                if let Ok(root) = crate::media_directory(&app) {
                    if let Ok(path) = partial_path(&root, &task) {
                        if let Ok(metadata) = tokio::fs::metadata(path).await {
                            progress(&app, &task.id, metadata.len(), task.total);
                        }
                    }
                }
            }
            crate::journal::record(
                "transfer",
                match &result {
                    Ok(_) => "录像已保存".to_string(),
                    Err(error) => format!("任务结束：{error}"),
                },
            );
            {
                let mut tasks = queue.tasks.lock().unwrap();
                if let Some(task) = tasks.iter_mut().find(|t| t.id == task.id) {
                    match result {
                        Ok(video) => {
                            task.received = video.size;
                            task.total = Some(video.size);
                            task.saved_path = video.saved_path.clone();
                            task.video = video;
                            task.status = "done".into();
                            task.error = task.video.export_error.clone();
                        }
                        Err(error) => {
                            if task.status == "downloading" {
                                task.status = "failed".into();
                                task.error = Some(error);
                            }
                        }
                    }
                }
                *queue.current.lock().unwrap() = None;
            }
            app.state::<crate::Transfers>()
                .0
                .lock()
                .await
                .remove(&task.id);
            queue.publish(&app).await;
        }
        #[cfg(target_os = "android")]
        let _ = crate::mobile::call(
            &app,
            "transfer",
            serde_json::json!({"active":false,"key":service_key}),
        )
        .await;
    });
}
fn needs_connection(task: &Task) -> bool {
    task.status == "waiting"
        || task.status == "downloading"
            && !matches!(task.phase.as_deref(), Some("masking" | "saving"))
}
pub async fn connection_lost(app: &AppHandle) -> Result<(), String> {
    let queue = app.state::<Arc<Queue>>();
    queue.load(app).await?;
    let active = {
        let mut tasks = queue.tasks.lock().unwrap();
        let mut active = Vec::new();
        for task in tasks.iter_mut().filter(|task| needs_connection(task)) {
            if task.status == "downloading" {
                active.push(task.id.clone());
            }
            task.status = "failed".into();
            task.error = Some("记录仪连接已断开，进度已保留，重新连接后可继续下载".into());
        }
        active
    };
    let state = app.state::<crate::Transfers>();
    let mut jobs = state.0.lock().await;
    for id in active {
        jobs.entry(id)
            .or_insert_with(|| Arc::new(AtomicBool::new(true)))
            .store(true, Ordering::Relaxed);
    }
    drop(jobs);
    queue.publish(app).await;
    Ok(())
}
pub fn progress(app: &AppHandle, id: &str, received: u64, total: Option<u64>) {
    let queue = app.state::<Arc<Queue>>();
    let snapshot = {
        let mut tasks = queue.tasks.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.received = received;
            task.total = total;
        }
        tasks.clone()
    };
    let _ = app.emit("transfer-state", snapshot);
}
pub fn phase(app: &AppHandle, id: &str, phase: &str) {
    let queue = app.state::<Arc<Queue>>();
    {
        if let Some(task) = queue
            .tasks
            .lock()
            .unwrap()
            .iter_mut()
            .find(|task| task.id == id)
        {
            task.phase = Some(phase.into());
        }
    }
    let _ = app.emit("transfer-state", queue.snapshot());
}
fn mark_cancelled(tasks: &mut [Task]) -> Vec<String> {
    let mut active = Vec::new();
    for task in tasks
        .iter_mut()
        .filter(|task| matches!(task.status.as_str(), "waiting" | "downloading"))
    {
        if task.status == "downloading" {
            active.push(task.id.clone());
        }
        task.status = "cancelled".into();
        task.error = None;
    }
    active
}
pub async fn cancel_all<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let queue = app.state::<Arc<Queue>>();
    queue.load(app).await?;
    let active = mark_cancelled(&mut queue.tasks.lock().unwrap());
    let state = app.state::<crate::Transfers>();
    let mut jobs = state.0.lock().await;
    for id in active {
        jobs.entry(id)
            .or_insert_with(|| Arc::new(AtomicBool::new(true)))
            .store(true, Ordering::Relaxed);
    }
    drop(jobs);
    queue.publish(app).await;
    crate::journal::record("transfer", "取消全部下载");
    Ok(())
}
pub async fn has_active(app: &AppHandle) -> Result<bool, String> {
    let queue = app.state::<Arc<Queue>>();
    queue.load(app).await?;
    Ok(queue
        .snapshot()
        .iter()
        .any(|task| matches!(task.status.as_str(), "waiting" | "downloading")))
}
#[tauri::command]
pub async fn control_transfers(app: AppHandle, action: String) -> Result<Vec<Task>, String> {
    match action.as_str() {
        "cancelAll" => cancel_all(&app).await?,
        "clearHistory" => {
            let queue = app.state::<Arc<Queue>>();
            queue.load(&app).await?;
            let _change = queue.changes.lock().await;
            let current = queue.current.lock().unwrap().clone();
            let removable: Vec<_> = queue
                .snapshot()
                .into_iter()
                .filter(|t| {
                    !matches!(t.status.as_str(), "waiting" | "downloading")
                        && current.as_ref() != Some(&t.id)
                })
                .collect();
            for task in &removable {
                cleanup(&app, task).await?;
            }
            queue
                .tasks
                .lock()
                .unwrap()
                .retain(|t| !removable.iter().any(|r| r.id == t.id));
            queue.publish(&app).await;
        }
        _ => return Err("不支持的队列操作".into()),
    }
    Ok(app.state::<Arc<Queue>>().snapshot())
}
#[tauri::command]
pub async fn list_transfers(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
) -> Result<Vec<Task>, String> {
    queue.load(&app).await?;
    Ok(queue.snapshot())
}
#[tauri::command]
pub async fn enqueue_transfers(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    address: String,
    videos: Vec<Video>,
) -> Result<Vec<Task>, String> {
    if videos.is_empty() || videos.len() > 100 {
        return Err("每次请选择 1–100 段录像".into());
    }
    let address = crate::protocol::authority(&address)?;
    let saved_ids: std::collections::HashSet<_> = crate::library::list(&app)
        .await?
        .into_iter()
        .map(|v| v.id)
        .collect();
    let mask_vin = crate::settings::read(&app).await?.mask_downloads;
    for video in &videos {
        crate::protocol::video_path(video)?;
    }
    queue.load(&app).await?;
    let _change = queue.changes.lock().await;
    {
        let mut tasks = queue.tasks.lock().unwrap();
        for mut video in videos {
            video.thumbnail = None;
            if tasks.iter().any(|task| {
                task.video.id == video.id
                    && (matches!(task.status.as_str(), "waiting" | "downloading")
                        || task.status == "done" && saved_ids.contains(&task.video.id))
            }) {
                continue;
            }
            if let Some(task) = tasks.iter_mut().find(|t| {
                t.video.id == video.id
                    && matches!(t.status.as_str(), "paused" | "failed" | "cancelled")
            }) {
                if queue.current.lock().unwrap().as_deref() == Some(task.id.as_str()) {
                    return Err("正在暂停下载，请稍后点击继续".into());
                }
                task.address = address.clone();
                task.status = "waiting".into();
                task.error = None;
                task.phase = None;
                continue;
            }
            tasks.push(Task {
                id: uuid::Uuid::new_v4().to_string(),
                address: address.clone(),
                total: Some(video.size).filter(|s| *s > 0),
                video,
                status: "waiting".into(),
                received: 0,
                error: None,
                saved_path: None,
                mask_vin,
                phase: None,
            });
        }
    }
    queue.publish(&app).await;
    let snapshot = queue.snapshot();
    kick(app, queue.inner().clone());
    Ok(snapshot)
}
#[tauri::command]
pub async fn cancel_transfer(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    task_id: String,
) -> Result<(), String> {
    queue.load(&app).await?;
    let downloading = {
        let mut tasks = queue.tasks.lock().unwrap();
        if let Some(task) = tasks
            .iter_mut()
            .find(|t| t.id == task_id && matches!(t.status.as_str(), "waiting" | "downloading"))
        {
            let downloading = task.status == "downloading";
            task.status = "paused".into();
            task.error = None;
            downloading
        } else {
            false
        }
    };
    if downloading {
        let state = app.state::<crate::Transfers>();
        state
            .0
            .lock()
            .await
            .entry(task_id)
            .or_insert_with(|| Arc::new(AtomicBool::new(true)))
            .store(true, Ordering::Relaxed);
    }
    queue.publish(&app).await;
    Ok(())
}
#[tauri::command]
pub async fn retry_transfer(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    task_id: String,
    address: String,
) -> Result<Vec<Task>, String> {
    queue.load(&app).await?;
    let address = crate::protocol::authority(&address)?;
    let _change = queue.changes.lock().await;
    {
        let mut tasks = queue.tasks.lock().unwrap();
        if queue.current.lock().unwrap().as_deref() == Some(task_id.as_str()) {
            return Err("正在停止上次下载，请稍后重试".into());
        }
        let task = tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or("没有找到传输任务")?;
        if !matches!(task.status.as_str(), "failed" | "cancelled" | "paused") {
            return Err("此任务不需要重试".into());
        }
        task.address = address;
        task.error = None;
        task.status = "waiting".into();
    }
    app.state::<crate::Transfers>()
        .0
        .lock()
        .await
        .remove(&task_id);
    queue.publish(&app).await;
    let snapshot = queue.snapshot();
    kick(app, queue.inner().clone());
    Ok(snapshot)
}

fn partial_path(root: &std::path::Path, task: &Task) -> Result<std::path::PathBuf, String> {
    if task.id.is_empty()
        || task.id.len() > 64
        || !task
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("任务标识无效".into());
    }
    let path = crate::protocol::video_path(&task.video)?;
    let name: String = path
        .rsplit('/')
        .next()
        .ok_or("缺少文件名")?
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(180)
        .collect();
    Ok(root
        .join(format!("{}_{}", task.id, name))
        .with_extension("part"))
}
async fn cleanup(app: &AppHandle, task: &Task) -> Result<(), String> {
    let partial = partial_path(&crate::media_directory(app)?, task)?;
    match tokio::fs::remove_file(&partial).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    crate::resume::remove_checkpoint(&partial).await;
    Ok(())
}
#[tauri::command]
pub async fn delete_transfer(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    task_id: String,
) -> Result<Vec<Task>, String> {
    queue.load(&app).await?;
    let _change = queue.changes.lock().await;
    {
        let mut tasks = queue.tasks.lock().unwrap();
        let task = tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or("没有找到传输任务")?;
        if matches!(task.status.as_str(), "waiting" | "downloading") {
            task.status = "paused".into();
            task.error = None;
        }
    }
    let active = queue.current.lock().unwrap().as_deref() == Some(task_id.as_str());
    if active {
        app.state::<crate::Transfers>()
            .0
            .lock()
            .await
            .entry(task_id.clone())
            .or_insert_with(|| Arc::new(AtomicBool::new(true)))
            .store(true, Ordering::Relaxed);
        queue.publish(&app).await;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while queue.current.lock().unwrap().as_deref() == Some(task_id.as_str()) {
            if std::time::Instant::now() > deadline {
                return Err("正在停止任务，请稍后再删除".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
    }
    if let Some(task) = queue.snapshot().into_iter().find(|t| t.id == task_id) {
        cleanup(&app, &task).await?;
    }
    queue.tasks.lock().unwrap().retain(|t| t.id != task_id);
    app.state::<crate::Transfers>()
        .0
        .lock()
        .await
        .remove(&task_id);
    queue.publish(&app).await;
    Ok(queue.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(status: &str) -> Task {
        serde_json::from_value(serde_json::json!({"id":status,"address":"192.168.43.8","status":status,"received":0,"total":100,"error":null,"savedPath":null,
            "video":{"id":"v","name":"v.mp4","path":"/mnt/mmc/Normal/v.mp4","category":"normal","date":"","time":"","size":100,"duration":null}})).unwrap()
    }
    #[test]
    fn cancel_all_stops_active_and_queued_but_keeps_completed() {
        let mut tasks = vec![
            task("waiting"),
            task("downloading"),
            task("done"),
            task("failed"),
        ];
        assert_eq!(mark_cancelled(&mut tasks), vec!["downloading"]);
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.status.as_str())
                .collect::<Vec<_>>(),
            vec!["cancelled", "cancelled", "done", "failed"]
        );
    }
    #[test]
    fn old_transfer_records_remain_readable_without_new_mask_fields() {
        let old = task("waiting");
        assert!(!old.mask_vin);
        assert!(old.phase.is_none());
    }
    #[test]
    fn completed_downloads_can_finish_local_masking_after_wifi_loss() {
        let mut local = task("downloading");
        local.phase = Some("masking".into());
        assert!(!needs_connection(&local));
        local.phase = Some("saving".into());
        assert!(!needs_connection(&local));
        local.phase = None;
        assert!(needs_connection(&local));
        assert!(needs_connection(&task("waiting")));
    }
}
