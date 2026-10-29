use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    at: u128,
    category: String,
    message: String,
}
#[derive(Default)]
struct Journal {
    file: Option<PathBuf>,
    entries: VecDeque<Entry>,
}
static JOURNAL: OnceLock<Mutex<Journal>> = OnceLock::new();

fn safe(message: &str) -> String {
    let lower = message.to_lowercase();
    if ["wifi:", "password", "密码", "authorization", "token="]
        .iter()
        .any(|key| lower.contains(key))
    {
        return "[已隐藏敏感信息]".into();
    }
    message
        .chars()
        .filter(|ch| *ch != '\0')
        .take(1200)
        .collect()
}
impl Journal {
    fn open(file: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
            .collect::<VecDeque<_>>();
        let mut journal = Self {
            file: Some(file),
            entries,
        };
        while journal.entries.len() > 500 {
            journal.entries.pop_front();
        }
        journal
    }
    fn push(&mut self, category: &str, message: &str) {
        let entry = Entry {
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            category: safe(category),
            message: safe(message),
        };
        self.entries.push_back(entry.clone());
        while self.entries.len() > 500 {
            self.entries.pop_front();
        }
        if let Some(file) = &self.file {
            let _ = std::fs::create_dir_all(file.parent().unwrap());
            if std::fs::metadata(file).is_ok_and(|metadata| metadata.len() > 512 * 1024) {
                // Keep recent history on rotation without retaining unlimited files.
                if let Ok(mut out) = std::fs::File::create(file) {
                    let mut recent = Vec::new();
                    let mut size = 0;
                    for entry in self.entries.iter().rev() {
                        let line = serde_json::to_string(entry).unwrap_or_default();
                        if size + line.len() + 1 > 256 * 1024 {
                            break;
                        }
                        size += line.len() + 1;
                        recent.push(line);
                    }
                    for line in recent.iter().rev() {
                        let _ = writeln!(out, "{line}");
                    }
                }
            } else if let Ok(mut out) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file)
            {
                let _ = writeln!(out, "{}", serde_json::to_string(&entry).unwrap_or_default());
            }
        }
    }
}
pub fn initialize(app: &AppHandle) {
    let file = app
        .path()
        .app_data_dir()
        .ok()
        .map(|root| root.join("diagnostics/operations.jsonl"));
    let _ = JOURNAL.set(Mutex::new(file.map(Journal::open).unwrap_or_default()));
    record("app", concat!("启动 v", env!("CARGO_PKG_VERSION")));
}
pub fn record(category: &str, message: impl AsRef<str>) {
    if let Some(journal) = JOURNAL.get() {
        if let Ok(mut journal) = journal.lock() {
            journal.push(category, message.as_ref());
        }
    }
}
async fn report(app: &AppHandle) -> Result<String, String> {
    let entries = JOURNAL
        .get()
        .and_then(|journal| journal.lock().ok().map(|journal| journal.entries.clone()))
        .unwrap_or_default();
    let native = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("diagnostics/android-crash.log");
    let native = std::fs::read_to_string(native).unwrap_or_default();
    let diagnostic = app.state::<crate::ConnectionState>().snapshot().ok();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let environment = crate::mobile::call(app, "environment", serde_json::json!({}))
        .await
        .unwrap_or(serde_json::Value::Null);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let environment =
        serde_json::json!({"os":std::env::consts::OS,"architecture":std::env::consts::ARCH});
    serde_json::to_string_pretty(&serde_json::json!({
        "application":"吉行影记", "version":app.package_info().version.to_string(), "platform":std::env::consts::OS,
        "notice":"本地日志；不记录热点密码、二维码、VIN 或视频内容。只有你主动选择分享时才交给其他应用。时间字段 at 为 Unix 毫秒。",
        "operations":entries,"connection":diagnostic,"environment":environment,"androidCrash":native.chars().take(64000).collect::<String>()
    })).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn get_operation_logs(app: AppHandle) -> Result<String, String> {
    report(&app).await
}
#[tauri::command]
pub fn record_client_event(category: String, message: String) -> Result<(), String> {
    if !["playback", "playback-error", "frontend-error"].contains(&category.as_str()) {
        return Err("不支持的日志类型".into());
    }
    record(&category, message);
    Ok(())
}
#[tauri::command]
pub async fn export_operation_logs(app: AppHandle, share: bool) -> Result<bool, String> {
    let text = report(&app).await?;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let result = crate::mobile::call(
            &app,
            if share { "shareLogs" } else { "saveLogs" },
            serde_json::json!({"text":text}),
        )
        .await?;
        Ok(result["cancelled"] != true)
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_dialog::DialogExt;
        let _ = share;
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_title("导出运行日志")
            .set_file_name("吉行影记日志.txt")
            .save_file(move |file| {
                let _ = tx.send(file);
            });
        let Some(file) = rx.await.map_err(|e| e.to_string())? else {
            return Ok(false);
        };
        tokio::fs::write(file.into_path().map_err(|e| e.to_string())?, text)
            .await
            .map_err(|e| e.to_string())?;
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_survives_restart_and_hides_credentials() {
        let file = std::env::temp_dir().join(format!("galaxy-log-{}.jsonl", uuid::Uuid::new_v4()));
        let mut journal = Journal::open(file.clone());
        journal.push("connection", "连接 192.168.43.8");
        journal.push("scan", "WIFI:S:car;P:secret;;");
        let loaded = Journal::open(file.clone());
        assert_eq!(loaded.entries.len(), 2);
        assert!(loaded.entries[0].message.contains("192.168.43.8"));
        assert!(!std::fs::read_to_string(&file).unwrap().contains("secret"));
        std::fs::remove_file(file).unwrap();
    }
    #[test]
    fn memory_history_and_messages_are_bounded() {
        let mut journal = Journal::default();
        for _ in 0..600 {
            journal.push("test", &"字".repeat(2000));
        }
        assert_eq!(journal.entries.len(), 500);
        assert_eq!(journal.entries[0].message.chars().count(), 1200);
    }
    #[test]
    fn disk_rotation_bounds_multibyte_logs_and_keeps_the_latest_entry() {
        let file = std::env::temp_dir().join(format!("galaxy-log-{}.jsonl", uuid::Uuid::new_v4()));
        let mut journal = Journal::open(file.clone());
        for index in 0..400 {
            journal.push("test", &format!("{index}:{}", "字".repeat(1190)));
        }
        assert!(std::fs::metadata(&file).unwrap().len() < 520 * 1024);
        let loaded = Journal::open(file.clone());
        assert!(loaded.entries.back().unwrap().message.starts_with("399:"));
        std::fs::remove_file(file).unwrap();
    }
}
