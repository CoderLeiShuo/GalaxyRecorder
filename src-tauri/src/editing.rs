#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::media::ffmpeg;
use crate::{media::MediaServer, protocol::Video};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
pub struct Mask {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Mask {
    pub fn validate(&self) -> Result<(), String> {
        if [self.x, self.y, self.width, self.height]
            .iter()
            .any(|v| !v.is_finite())
            || self.x < 0.0
            || self.y < 0.0
            || self.width <= 0.0
            || self.height <= 0.0
            || self.x + self.width > 1.0
            || self.y + self.height > 1.0
        {
            return Err("遮挡区域必须位于画面内".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Exports {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    operation: Mutex<()>,
}
pub fn busy(app: &AppHandle) -> bool {
    app.state::<Exports>().operation.try_lock().is_err()
}
async fn cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
async fn encode(
    app: &AppHandle,
    source: &str,
    output: &std::path::Path,
    start: f64,
    end: Option<f64>,
    mask: Option<&Mask>,
    cancel: Arc<AtomicBool>,
    background: bool,
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("导出已取消".into());
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let result = tokio::select! {
            result = crate::mobile::call(app,"export",serde_json::json!({"source":source,"output":output,"start":start,"end":end,"mask":mask,"background":background})) => result.map(|_| ()),
            _ = cancelled(&cancel) => Err("导出已取消".into()),
        };
        if result.is_err() {
            let _ = crate::mobile::call(app, "cancelExport", serde_json::json!({})).await;
        }
        result?;
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = (app, background);
        let mut command = tokio::process::Command::new(ffmpeg());
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            source,
            "-ss",
            &start.to_string(),
        ]);
        if let Some(end) = end {
            command.args(["-t", &(end - start).to_string()]);
        }
        if let Some(mask) = mask {
            command.args([
                "-vf",
                &format!(
                    "drawbox=x=floor(iw*{}):y=floor(ih*{}):w=ceil(iw*{})+2:h=ceil(ih*{})+2:color=black@1:t=fill",
                    mask.x, mask.y, mask.width, mask.height
                ),
            ]);
        }
        command
            .args([
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "18",
                "-c:a",
                "aac",
                "-movflags",
                "+faststart",
                "-y",
            ])
            .arg(output);
        let mut child = command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "导出需要可用的 FFmpeg")?;
        let status = tokio::select! { status=child.wait()=>status.map_err(|e|e.to_string())?, _=cancelled(&cancel)=>return Err("导出已取消".into()) };
        if !status.success() {
            return Err("导出失败，请检查录像编码与片段范围".into());
        }
    }
    Ok(())
}
fn default_vin_mask() -> Mask {
    Mask {
        x: 0.62,
        y: 0.0,
        width: 0.38,
        height: 0.09,
    }
}
pub async fn mask_download(
    app: &AppHandle,
    mut video: Video,
    cancel: Arc<AtomicBool>,
) -> Result<Video, String> {
    let state = app.state::<Exports>();
    let _operation = tokio::select! { guard=state.operation.lock()=>guard, _=cancelled(&cancel)=>return Err("下载已取消".into()) };
    let input = std::path::PathBuf::from(video.saved_path.as_ref().ok_or("原片未下载完成")?);
    let output = input.with_extension("masked.mp4");
    let temporary = input.with_extension("masked.part.mp4");
    let source = app.state::<MediaServer>().video(None, &video, app).await?;
    let mask = default_vin_mask();
    let result = async {
        encode(
            app,
            &source.url,
            &temporary,
            0.0,
            None,
            Some(&mask),
            cancel.clone(),
            true,
        )
        .await?;
        if cancel.load(Ordering::Relaxed) {
            return Err("下载已取消".into());
        }
        tokio::fs::rename(&temporary, &output)
            .await
            .map_err(|e| e.to_string())?;
        video.saved_path = Some(output.to_string_lossy().into());
        video.storage = None;
        video.thumbnail = None;
        video.exported_path = None;
        video.name = format!("已打码_{}", video.name);
        video.size = tokio::fs::metadata(&output)
            .await
            .map_err(|e| e.to_string())?
            .len();
        Ok(video)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
        let _ = tokio::fs::remove_file(&output).await;
    }
    result
}
#[tauri::command]
pub async fn cancel_export(
    app: AppHandle,
    state: State<'_, Exports>,
    job_id: String,
) -> Result<(), String> {
    let active = if let Some(cancel) = state.jobs.lock().await.get(&job_id) {
        cancel.store(true, Ordering::Relaxed);
        true
    } else {
        false
    };
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if active {
            crate::mobile::call(&app, "cancelExport", serde_json::json!({})).await?;
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = (app, active);
    Ok(())
}
#[tauri::command]
pub async fn export_video(
    app: AppHandle,
    address: Option<String>,
    mut video: Video,
    start: f64,
    end: Option<f64>,
    mask: Option<Mask>,
    job_id: String,
    server: State<'_, MediaServer>,
    exports: State<'_, Exports>,
) -> Result<Video, String> {
    if video.saved_path.is_some() {
        video = crate::library::find(&app, &video.id).await?;
    }
    if !start.is_finite()
        || start < 0.0
        || end.is_some_and(|end| !end.is_finite() || end <= start || end - start > 86400.0)
    {
        return Err("请选择有效的片段范围".into());
    }
    if video
        .duration
        .is_some_and(|duration| end.is_some_and(|end| end > duration + 0.5) || start >= duration)
    {
        return Err("片段范围超出录像时长".into());
    }
    if let Some(mask) = &mask {
        mask.validate()?;
    }
    let _operation = exports
        .operation
        .try_lock()
        .map_err(|_| "请等待当前导出完成")?;
    crate::journal::record(
        "export",
        format!(
            "导出片段，开始={start}，结束={end:?}，打码={}",
            mask.is_some()
        ),
    );
    if video.saved_path.is_none() {
        crate::network::prepare(&app, false).await?;
    }
    let source = server.video(address.as_deref(), &video, &app).await?;
    let directory = crate::media_directory(&app)?;
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let output = directory.join(format!("export_{id}.mp4"));
    let temporary = output.with_extension("part.mp4");
    let cancel = Arc::new(AtomicBool::new(false));
    exports
        .jobs
        .lock()
        .await
        .insert(job_id.clone(), cancel.clone());
    let result = async {
        encode(
            &app,
            &source.url,
            &temporary,
            start,
            end,
            mask.as_ref(),
            cancel.clone(),
            false,
        )
        .await?;
        if cancel.load(Ordering::Relaxed) {
            return Err("导出已取消".into());
        }
        tokio::fs::rename(&temporary, &output)
            .await
            .map_err(|e| e.to_string())?;
        video.id = format!("export:{id}");
        video.name = format!(
            "{}_{}.mp4",
            if mask.is_some() {
                "已遮挡车架号"
            } else {
                "片段"
            },
            video.name.trim_end_matches(".mp4")
        );
        video.size = tokio::fs::metadata(&output)
            .await
            .map_err(|e| e.to_string())?
            .len();
        video.duration = end
            .map(|end| end - start)
            .or_else(|| video.duration.map(|duration| duration - start));
        video.saved_path = Some(output.to_string_lossy().into());
        video.storage = None;
        video.exported_path = None;
        // Exported thumbnails must come from the new frames, including any applied mask.
        video.thumbnail = None;
        crate::library::store(&app, video, true).await
    }
    .await;
    exports.jobs.lock().await.remove(&job_id);
    crate::journal::record(
        "export",
        match &result {
            Ok(_) => "片段导出完成".to_string(),
            Err(error) => format!("导出未完成：{error}"),
        },
    );
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
        let _ = tokio::fs::remove_file(&output).await;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_mask_covers_the_upper_right_edge() {
        let mask = default_vin_mask();
        mask.validate().unwrap();
        assert_eq!(mask.x + mask.width, 1.0);
        assert_eq!(mask.y, 0.0);
        assert!(mask.height >= 0.08);
    }
    #[test]
    fn mask_stays_inside_the_frame() {
        assert!(Mask {
            x: 0.73,
            y: 0.015,
            width: 0.21,
            height: 0.045
        }
        .validate()
        .is_ok());
        assert!(Mask {
            x: 0.9,
            y: 0.0,
            width: 0.2,
            height: 0.1
        }
        .validate()
        .is_err());
        assert!(Mask {
            x: f64::NAN,
            y: 0.0,
            width: 0.1,
            height: 0.1
        }
        .validate()
        .is_err());
    }
}
