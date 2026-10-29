use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
static SETTINGS_WRITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub connection_profile: String,
    pub last_device: Option<crate::vehicle::DeviceMemory>,
    pub known_devices: Vec<crate::vehicle::DeviceMemory>,
    pub save_mode: String,
    pub display_name: String,
    pub device_title: String,
    pub device_image: Option<String>,
    pub vehicle_aliases: std::collections::BTreeMap<String, String>,
    pub directory: Option<String>,
    pub directory_label: Option<String>,
    pub mask_by_default: bool,
    pub mask_downloads: bool,
    pub keep_screen_on: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            connection_profile: "AUTO".into(),
            last_device: None,
            known_devices: Vec::new(),
            save_mode: "gallery".into(),
            display_name: "吉行影记".into(),
            device_title: "我的记录仪".into(),
            device_image: None,
            vehicle_aliases: Default::default(),
            directory: None,
            directory_label: None,
            mask_by_default: false,
            mask_downloads: false,
            keep_screen_on: true,
        }
    }
}
impl Settings {
    fn migrate_legacy_defaults(&mut self) {
        // Downloads always publish to the system gallery; keep folder grants for explicit exports.
        self.save_mode = "gallery".into();
        self.keep_screen_on = true;
        if self.display_name == "银河记录仪" {
            self.display_name = "吉行影记".into();
        }
    }
}
fn path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("settings.json"))
        .map_err(|e| e.to_string())
}
pub async fn read(app: &AppHandle) -> Result<Settings, String> {
    match tokio::fs::read(path(app)?).await {
        Ok(bytes) => {
            let mut settings: Settings =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            settings.migrate_legacy_defaults();
            Ok(settings)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e.to_string()),
    }
}
async fn write(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = path(app)?;
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .map_err(|e| e.to_string())?;
    let temporary = path.with_extension("json.part");
    tokio::fs::write(
        &temporary,
        serde_json::to_vec(settings).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;
    tokio::fs::rename(temporary, path)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    read(&app).await
}
#[tauri::command]
pub async fn save_settings(app: AppHandle, mut settings: Settings) -> Result<Settings, String> {
    if !["app", "folder", "gallery"].contains(&settings.save_mode.as_str()) {
        return Err("不支持的保存方式".into());
    }
    let _settings = SETTINGS_WRITE.lock().await;
    // Directory grants come only from the native picker, never from free text input.
    let old = read(&app).await?;
    settings.display_name = old.display_name;
    settings.device_title = old.device_title;
    settings.device_image = old.device_image;
    settings.vehicle_aliases = old.vehicle_aliases;
    settings.connection_profile = old.connection_profile;
    settings.last_device = old.last_device;
    settings.known_devices = old.known_devices;
    settings.save_mode = "gallery".into();
    settings.keep_screen_on = true;
    settings.directory = old.directory;
    settings.directory_label = old.directory_label;
    if settings.save_mode == "folder" && settings.directory.is_none() {
        return Err("请先选择保存目录".into());
    }
    write(&app, &settings).await?;
    Ok(settings)
}
pub fn valid_display_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 32 || name.chars().any(char::is_control) {
        return Err("名称请输入 1–32 个可见字符".into());
    }
    Ok(name.to_string())
}
#[tauri::command]
pub async fn rename_app(app: AppHandle, name: String) -> Result<Settings, String> {
    let _settings = SETTINGS_WRITE.lock().await;
    let mut settings = read(&app).await?;
    settings.display_name = valid_display_name(&name)?;
    write(&app, &settings).await?;
    Ok(settings)
}

#[tauri::command]
pub async fn choose_download_directory(app: AppHandle) -> Result<Option<Settings>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let selection = {
        let value = crate::mobile::call(&app, "chooseDirectory", serde_json::json!({})).await?;
        value["uri"].as_str().map(|uri| {
            (
                uri.to_string(),
                value["label"].as_str().unwrap_or("已授权目录").to_string(),
            )
        })
    };
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let selection = {
        use tauri_plugin_dialog::DialogExt;
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_title("选择录像保存文件夹")
            .pick_folder(move |folder| {
                let _ = tx.send(folder);
            });
        rx.await
            .map_err(|e| e.to_string())?
            .map(|folder| {
                folder.into_path().map(|p| {
                    let label = p.to_string_lossy().to_string();
                    (label.clone(), label)
                })
            })
            .transpose()
            .map_err(|e| e.to_string())?
    };
    let Some((directory, label)) = selection else {
        return Ok(None);
    };
    let _settings = SETTINGS_WRITE.lock().await;
    let mut settings = read(&app).await?;
    settings.directory = Some(directory);
    settings.directory_label = Some(label);
    settings.save_mode = "gallery".into();
    write(&app, &settings).await?;
    Ok(Some(settings))
}
pub async fn export(
    app: &AppHandle,
    source: &str,
    name: &str,
    mode: Option<&str>,
) -> Result<Option<String>, String> {
    let settings = read(app).await?;
    let mode = mode.unwrap_or(&settings.save_mode);
    let mode = if mode == "app" { "gallery" } else { mode };
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let result=crate::mobile::call(app,"save",serde_json::json!({"path":source,"name":name,"mode":mode,"directory":settings.directory})).await?;
        return Ok(result["uri"].as_str().map(str::to_string));
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let directory = if mode == "gallery" {
            app.path()
                .video_dir()
                .map_err(|e| e.to_string())?
                .join("GalaxyRecorder")
        } else {
            PathBuf::from(settings.directory.ok_or("请先选择保存目录")?)
        };
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|e| e.to_string())?;
        let name = std::path::Path::new(name)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("无效文件名")?;
        let target = directory.join(format!("{}_{}", uuid::Uuid::new_v4().simple(), name));
        tokio::fs::copy(source, &target)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Some(target.to_string_lossy().into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_installs_save_directly_to_gallery() {
        assert_eq!(Settings::default().save_mode, "gallery");
    }
    #[test]
    fn names_preserve_unicode_and_reject_invalid_input() {
        assert_eq!(
            valid_display_name("  小雷的星越 L  ").unwrap(),
            "小雷的星越 L"
        );
        for name in ["", "  ", "bad\nname"] {
            assert!(valid_display_name(name).is_err());
        }
        assert!(valid_display_name(&"星".repeat(33)).is_err());
    }
    #[test]
    fn rebranding_updates_the_old_default_and_preserves_user_names_and_files() {
        let mut old: Settings = serde_json::from_str(
            r#"{"displayName":"银河记录仪","directory":"/saved/car","saveMode":"folder"}"#,
        )
        .unwrap();
        old.migrate_legacy_defaults();
        assert_eq!(old.display_name, "吉行影记");
        assert_eq!(old.directory.as_deref(), Some("/saved/car"));
        assert_eq!(old.save_mode, "gallery");
        old.display_name = "我的座驾".into();
        old.migrate_legacy_defaults();
        assert_eq!(old.display_name, "我的座驾");
        assert_eq!(Settings::default().display_name, "吉行影记");
    }
    #[test]
    fn old_folder_and_screen_preferences_upgrade_to_fixed_defaults() {
        let mut settings: Settings = serde_json::from_str(
            r#"{"saveMode":"folder","keepScreenOn":false,"directory":"/old/recordings"}"#,
        )
        .unwrap();
        settings.migrate_legacy_defaults();
        assert_eq!(settings.save_mode, "gallery");
        assert!(settings.keep_screen_on);
        assert_eq!(settings.directory.as_deref(), Some("/old/recordings"));
    }
    #[test]
    fn older_settings_do_not_enable_download_masking() {
        let old: Settings =
            serde_json::from_str(r#"{"saveMode":"app","maskByDefault":true}"#).unwrap();
        assert!(old.mask_by_default);
        assert!(!old.mask_downloads);
    }
}

#[tauri::command]
pub async fn set_connection_profile(app: AppHandle, profile: String) -> Result<Settings, String> {
    let profile = crate::adapters::profile(Some(&profile))?;
    let _guard = SETTINGS_WRITE.lock().await;
    let mut settings = read(&app).await?;
    settings.connection_profile = profile.into();
    write(&app, &settings).await?;
    Ok(settings)
}
pub async fn remember_device(
    app: &AppHandle,
    mut memory: crate::vehicle::DeviceMemory,
) -> Result<crate::vehicle::DeviceMemory, String> {
    let _guard = SETTINGS_WRITE.lock().await;
    let mut settings = read(app).await?;
    let old = settings
        .known_devices
        .iter()
        .find(|d| d.key == memory.key)
        .and_then(|d| d.vehicle.as_ref());
    memory.vehicle = crate::vehicle::merge(old, memory.vehicle);
    settings.known_devices.retain(|d| d.key != memory.key);
    settings.known_devices.push(memory.clone());
    if settings.known_devices.len() > 12 {
        settings.known_devices.remove(0);
    }
    settings.last_device = Some(memory.clone());
    write(app, &settings).await?;
    Ok(memory)
}
#[tauri::command]
pub async fn confirm_vehicle(
    app: AppHandle,
    device_key: String,
    model: String,
    year: Option<u16>,
    configuration: Option<String>,
) -> Result<crate::vehicle::DeviceMemory, String> {
    let vehicle = crate::vehicle::confirmed(&model, year, configuration)?;
    let _guard = SETTINGS_WRITE.lock().await;
    let mut settings = read(&app).await?;
    let memory = settings
        .known_devices
        .iter_mut()
        .find(|d| d.key == device_key)
        .ok_or("请先连接一次记录仪，再确认车型")?;
    memory.vehicle = Some(vehicle);
    let memory = memory.clone();
    if settings
        .last_device
        .as_ref()
        .is_some_and(|d| d.key == device_key)
    {
        settings.last_device = Some(memory.clone());
    }
    write(&app, &settings).await?;
    Ok(memory)
}

fn validate_device_image(value: &str) -> Result<(), String> {
    use base64::Engine;
    if value.len() > 4 * 1024 * 1024 {
        return Err("图片内容过大".into());
    }
    let encoded = value
        .strip_prefix("data:image/png;base64,")
        .ok_or("请使用 PNG 图片")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "图片数据无效")?;
    if bytes.len() < 33 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err("图片格式无效".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 || width > 1200 || height > 1200 {
        return Err("图片尺寸无效".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn save_appearance(
    app: AppHandle,
    kind: String,
    value: Option<String>,
    device_key: Option<String>,
) -> Result<Settings, String> {
    let _guard = SETTINGS_WRITE.lock().await;
    let mut settings = read(&app).await?;
    apply_appearance(&mut settings, &kind, value, device_key)?;
    write(&app, &settings).await?;
    Ok(settings)
}

fn apply_appearance(
    settings: &mut Settings,
    kind: &str,
    value: Option<String>,
    device_key: Option<String>,
) -> Result<(), String> {
    match kind {
        "title" => {
            settings.device_title = value
                .as_deref()
                .map(valid_display_name)
                .transpose()?
                .unwrap_or_else(|| "我的记录仪".into())
        }
        "image" => {
            if let Some(ref data) = value {
                validate_device_image(data)?;
            }
            settings.device_image = value;
        }
        "vehicle" => {
            let key = device_key.ok_or("请先连接一次记录仪")?;
            if !settings
                .known_devices
                .iter()
                .any(|device| device.key == key)
            {
                return Err("未找到保存的车辆".into());
            }
            if let Some(name) = value {
                settings
                    .vehicle_aliases
                    .insert(key, valid_display_name(&name)?);
            } else {
                settings.vehicle_aliases.remove(&key);
            }
        }
        _ => return Err("不支持的外观设置".into()),
    }
    Ok(())
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    use crate::vehicle::{DeviceMemory, VehicleInfo};
    #[test]
    fn aliases_survive_settings_roundtrip_without_changing_identified_vehicle() {
        let mut settings = Settings::default();
        let memory = DeviceMemory {
            key: "device-car".into(),
            address: "192.168.43.8".into(),
            profile: "A7".into(),
            vehicle: Some(VehicleInfo {
                name: "星越L2026款".into(),
                model: "星越L".into(),
                year: Some(2026),
                configuration: Some("旗舰型".into()),
                source: "device".into(),
            }),
        };
        settings.known_devices.push(memory.clone());
        settings.last_device = Some(memory);
        apply_appearance(
            &mut settings,
            "vehicle",
            Some("我的星越".into()),
            Some("device-car".into()),
        )
        .unwrap();
        apply_appearance(&mut settings, "title", Some("行车伙伴".into()), None).unwrap();
        let mut restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.device_title, "行车伙伴");
        assert_eq!(restored.vehicle_aliases["device-car"], "我的星越");
        assert_eq!(
            restored
                .last_device
                .as_ref()
                .unwrap()
                .vehicle
                .as_ref()
                .unwrap()
                .name,
            "星越L2026款"
        );
        apply_appearance(&mut restored, "vehicle", None, Some("device-car".into())).unwrap();
        assert!(restored.vehicle_aliases.is_empty());
        assert!(apply_appearance(
            &mut restored,
            "vehicle",
            Some("别的车".into()),
            Some("device-unknown".into())
        )
        .is_err());
    }
    #[test]
    fn imported_png_is_bounded_and_reset_keeps_other_preferences() {
        let png="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==";
        let mut settings = Settings::default();
        settings.connection_profile = "L6".into();
        apply_appearance(&mut settings, "image", Some(png.into()), None).unwrap();
        assert_eq!(settings.device_image.as_deref(), Some(png));
        for bad in [
            "data:image/svg+xml,<svg/>",
            "data:image/png;base64,bad",
            "/outside/file.png",
        ] {
            assert!(apply_appearance(&mut settings, "image", Some(bad.into()), None).is_err());
            assert_eq!(settings.device_image.as_deref(), Some(png));
        }
        apply_appearance(&mut settings, "image", None, None).unwrap();
        assert!(settings.device_image.is_none());
        assert_eq!(settings.connection_profile, "L6");
    }
    #[test]
    fn existing_install_gets_original_title_and_picture() {
        let settings: Settings =
            serde_json::from_str(r#"{"displayName":"银河记录仪","saveMode":"gallery"}"#).unwrap();
        assert_eq!(settings.device_title, "我的记录仪");
        assert!(settings.device_image.is_none());
        assert!(settings.vehicle_aliases.is_empty());
    }
}
