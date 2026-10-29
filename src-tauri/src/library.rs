use crate::protocol::Video;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use tauri::AppHandle;

pub fn external(video: &Video) -> bool {
    matches!(video.storage.as_deref(), Some("gallery" | "folder"))
}
fn record_path(root: &Path, id: &str) -> PathBuf {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    root.join(format!("saved_{:016x}.json", hash.finish()))
}
async fn write_record(root: &Path, video: &Video) -> Result<(), String> {
    tokio::fs::create_dir_all(root)
        .await
        .map_err(|e| e.to_string())?;
    let path = record_path(root, &video.id);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        if let Ok(old) = serde_json::from_slice::<Video>(&bytes) {
            if old.id != video.id {
                return Err("录像索引冲突，未覆盖已有文件".into());
            }
        }
    }
    let temp = path.with_extension("json.part");
    tokio::fs::write(&temp, serde_json::to_vec(video).map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(temp, path)
        .await
        .map_err(|e| e.to_string())
}
pub async fn records(root: &Path) -> Result<Vec<(PathBuf, Video)>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(root).await.map_err(|e| e.to_string())?;
    let mut found: HashMap<String, (PathBuf, Video)> = HashMap::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(video) = tokio::fs::read(&path)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice::<Video>(&b).ok())
        else {
            continue;
        };
        let Some(saved) = video.saved_path.as_deref() else {
            continue;
        };
        if !external(&video) {
            let Ok(file) = tokio::fs::canonicalize(saved).await else {
                continue;
            };
            let canonical = tokio::fs::canonicalize(root)
                .await
                .map_err(|e| e.to_string())?;
            if file.parent() != Some(canonical.as_path()) || !file.is_file() {
                continue;
            }
        }
        let modern = path == record_path(root, &video.id);
        if modern || !found.contains_key(&video.id) {
            found.insert(video.id.clone(), (path, video));
        }
    }
    Ok(found.into_values().collect())
}
pub async fn list(app: &AppHandle) -> Result<Vec<Video>, String> {
    let mut videos: Vec<_> = records(&crate::media_directory(app)?)
        .await?
        .into_iter()
        .map(|(_, v)| v)
        .collect();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let paths: Vec<_> = videos
            .iter()
            .filter(|v| external(v))
            .filter_map(|v| v.saved_path.clone())
            .collect();
        if !paths.is_empty() {
            let state =
                crate::mobile::call(app, "inspectSavedMany", serde_json::json!({"paths":paths}))
                    .await?;
            videos.retain(|video| {
                !external(video)
                    || state["items"].as_array().is_some_and(|items| {
                        items.iter().any(|item| {
                            item["path"].as_str() == video.saved_path.as_deref()
                                && item["exists"] == true
                        })
                    })
            });
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    videos.retain(|v| {
        v.saved_path
            .as_ref()
            .is_some_and(|p| Path::new(p).is_file())
    });
    videos.sort_by(|a, b| b.date.cmp(&a.date).then(b.time.cmp(&a.time)));
    Ok(videos)
}
pub async fn find(app: &AppHandle, id: &str) -> Result<Video, String> {
    list(app)
        .await?
        .into_iter()
        .find(|v| v.id == id)
        .ok_or_else(|| "没有找到已保存录像，文件可能已在相册中删除".into())
}
// Commit the external record before removing the temporary source. A failed index write keeps the source recoverable.
async fn commit_external(root: &Path, video: &Video, source: &Path) -> Result<(), String> {
    write_record(root, video).await?;
    tokio::fs::remove_file(source)
        .await
        .map_err(|e| format!("已保存，临时文件清理失败：{e}"))?;
    let legacy = source.with_extension("json");
    if legacy != record_path(root, &video.id) {
        let _ = tokio::fs::remove_file(legacy).await;
    }
    Ok(())
}
async fn publish(app: &AppHandle, mut video: Video, mode: Option<&str>) -> Result<Video, String> {
    let source = video.saved_path.clone().ok_or("录像尚未保存")?;
    let settings = crate::settings::read(app).await?;
    let mode = mode.unwrap_or(&settings.save_mode);
    let mode = if mode == "app" { "gallery" } else { mode };
    let destination = crate::settings::export(app, &source, &video.name, Some(mode))
        .await?
        .ok_or("保存位置未返回文件，请重试")?;
    let was_external = external(&video);
    video.saved_path = Some(destination.clone());
    video.exported_path = Some(destination);
    video.storage = Some(mode.to_string());
    video.export_error = None;
    if was_external {
        write_record(&crate::media_directory(app)?, &video).await?;
    } else if let Err(error) =
        commit_external(&crate::media_directory(app)?, &video, Path::new(&source)).await
    {
        // If the record exists, the destination is published; do not create another gallery file on retry.
        let published = tokio::fs::read(record_path(&crate::media_directory(app)?, &video.id))
            .await
            .ok()
            .and_then(|b| serde_json::from_slice::<Video>(&b).ok())
            .is_some_and(|v| v.saved_path == video.saved_path);
        if !published {
            #[cfg(any(target_os = "android", target_os = "ios"))]
            let _ = crate::mobile::call(
                app,
                "deleteSaved",
                serde_json::json!({"path":video.saved_path}),
            )
            .await;
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if let Some(path) = &video.saved_path {
                let _ = tokio::fs::remove_file(path).await;
            }
            return Err(error);
        }
        video.export_error = Some(error);
        write_record(&crate::media_directory(app)?, &video).await?;
    }
    crate::journal::record("library", "录像已保存到所选位置，保留文件索引");
    Ok(video)
}
pub async fn store(app: &AppHandle, video: Video, export: bool) -> Result<Video, String> {
    if export {
        publish(app, video, None).await
    } else {
        write_record(&crate::media_directory(app)?, &video).await?;
        Ok(video)
    }
}
#[tauri::command]
pub async fn export_saved(app: AppHandle, id: String, mode: String) -> Result<Video, String> {
    if !["folder", "gallery"].contains(&mode.as_str()) {
        return Err("请选择系统相册或已授权目录".into());
    }
    let video = find(&app, &id).await?;
    if video.storage.as_deref() == Some(mode.as_str()) {
        return Ok(video);
    }
    if !external(&video) {
        return publish(&app, video, Some(&mode)).await;
    }
    let mut video = video;
    video.exported_path = crate::settings::export(
        &app,
        video.saved_path.as_deref().unwrap(),
        &video.name,
        Some(&mode),
    )
    .await?;
    video.export_error = None;
    store(&app, video, false).await
}
#[tauri::command]
pub async fn delete_saved(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    if ids.len() > 100 {
        return Err("单次最多删除 100 段录像".into());
    }
    for id in ids {
        let video = find(&app, &id).await?;
        let path = video.saved_path.as_deref().unwrap();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        if external(&video) {
            crate::mobile::call(&app, "deleteSaved", serde_json::json!({"path":path})).await?;
        } else {
            tokio::fs::remove_file(path)
                .await
                .map_err(|e| e.to_string())?;
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| e.to_string())?;
        let root = crate::media_directory(&app)?;
        let _ = tokio::fs::remove_file(record_path(&root, &id)).await;
        if !external(&video) {
            let _ = tokio::fs::remove_file(Path::new(path).with_extension("json")).await;
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn rename_saved(app: AppHandle, id: String, name: String) -> Result<Video, String> {
    let mut video = find(&app, &id).await?;
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 128
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || name == "."
        || name == ".."
    {
        return Err("请输入 1–128 个字符的文件名，不能包含路径分隔符".into());
    }
    let name = if name.to_lowercase().ends_with(".mp4") {
        name.to_string()
    } else {
        format!("{name}.mp4")
    };
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if external(&video) {
        let result = crate::mobile::call(
            &app,
            "renameSaved",
            serde_json::json!({"path":video.saved_path,"name":name}),
        )
        .await?;
        video.saved_path = Some(result["uri"].as_str().ok_or("重命名未返回文件")?.into());
    }
    if !external(&video) {
        let source = PathBuf::from(video.saved_path.as_ref().unwrap());
        let target = source.parent().ok_or("录像路径无效")?.join(&name);
        if source != target {
            if target.exists() {
                return Err("同名文件已存在".into());
            }
            tokio::fs::rename(&source, &target)
                .await
                .map_err(|e| e.to_string())?;
            video.saved_path = Some(target.to_string_lossy().into());
            video.name = name;
            match store(&app, video, false).await {
                Ok(video) => {
                    let _ = tokio::fs::remove_file(source.with_extension("json")).await;
                    return Ok(video);
                }
                Err(error) => {
                    let _ = tokio::fs::rename(target, source).await;
                    return Err(error);
                }
            }
        }
    }
    video.name = name;
    store(&app, video, false).await
}
#[tauri::command]
pub async fn share_saved(app: AppHandle, id: String) -> Result<(), String> {
    share_saved_many(app, vec![id], false).await
}
#[tauri::command]
pub async fn share_saved_many(
    app: AppHandle,
    ids: Vec<String>,
    original: bool,
) -> Result<(), String> {
    if ids.is_empty() || ids.len() > 100 {
        return Err("请选择 1–100 段已保存录像".into());
    }
    let saved = list(&app).await?;
    let mut paths = Vec::new();
    for id in ids {
        let video = saved
            .iter()
            .find(|v| v.id == id)
            .ok_or("部分录像已不存在，请刷新后重试")?;
        let path = video.saved_path.clone().ok_or("录像尚未下载")?;
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    crate::mobile::call(
        &app,
        "shareMany",
        serde_json::json!({"paths":paths,"original":original}),
    )
    .await?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_opener::OpenerExt;
        let _ = original;
        for path in paths {
            app.opener()
                .reveal_item_in_dir(path)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn video(path: &Path) -> Video {
        serde_json::from_value(serde_json::json!({"id":"normal:fixture","name":"v.mp4","path":"/mnt/mmc/Normal/v.mp4","category":"normal","date":"","time":"","size":16,"duration":null,"savedPath":path.to_str().unwrap()})).unwrap()
    }
    #[tokio::test]
    async fn legacy_files_remain_readable_but_outside_paths_are_rejected() {
        let root = std::env::temp_dir().join(format!("galaxy-legacy-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let source = root.join("old.mp4");
        tokio::fs::write(&source, b"old recording").await.unwrap();
        let old = video(&source);
        tokio::fs::write(
            source.with_extension("json"),
            serde_json::to_vec(&old).unwrap(),
        )
        .await
        .unwrap();
        let outside = root.with_extension("mp4");
        tokio::fs::write(&outside, b"outside").await.unwrap();
        let mut bad = video(&outside);
        bad.id = "invalid".into();
        tokio::fs::write(root.join("outside.json"), serde_json::to_vec(&bad).unwrap())
            .await
            .unwrap();
        let files = records(&root).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.saved_path, old.saved_path);
        assert!(source.exists());
        tokio::fs::remove_file(outside).await.unwrap();
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn publishing_keeps_only_external_file_and_index() {
        let root = std::env::temp_dir().join(format!("galaxy-publish-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let source = root.join("v.mp4");
        tokio::fs::write(&source, b"fixture").await.unwrap();
        let mut v = video(&source);
        v.saved_path = Some("content://media/external/video/media/18".into());
        v.storage = Some("gallery".into());
        commit_external(&root, &v, &source).await.unwrap();
        assert!(!source.exists());
        assert_eq!(records(&root).await.unwrap()[0].1.saved_path, v.saved_path);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn failed_index_commit_keeps_download_recoverable() {
        let root = std::env::temp_dir().join(format!("galaxy-publish-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let source = root.join("v.mp4");
        tokio::fs::write(&source, b"fixture").await.unwrap();
        let v = video(&source);
        tokio::fs::create_dir(record_path(&root, &v.id))
            .await
            .unwrap();
        assert!(commit_external(&root, &v, &source).await.is_err());
        assert_eq!(tokio::fs::read(&source).await.unwrap(), b"fixture");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
