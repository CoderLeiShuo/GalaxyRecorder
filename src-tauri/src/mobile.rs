#[cfg(any(target_os = "android", target_os = "ios"))]
use tauri::Manager;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};
#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_recorder_media);
#[cfg(any(target_os = "android", target_os = "ios"))]
struct Mobile<R: Runtime>(tauri::plugin::PluginHandle<R>);
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("recorder-media")
        .setup(|app, api| {
            #[cfg(target_os = "ios")]
            app.manage(Mobile(api.register_ios_plugin(init_plugin_recorder_media)?));
            #[cfg(target_os = "android")]
            {
                let plugin = api
                    .register_android_plugin("com.leishuo.galaxyrecorder", "RecorderMediaPlugin")?;
                let receiver = app.clone();
                let channel = tauri::ipc::Channel::<serde_json::Value>::new(move |body| {
                    if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                        if serde_json::from_str::<serde_json::Value>(&json)
                            .ok()
                            .is_some_and(|value| value["action"] == "cancelAll")
                        {
                            let app = receiver.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = crate::transfers::cancel_all(&app).await;
                            });
                        }
                    }
                    Ok(())
                });
                let listener = plugin.clone();
                tauri::async_runtime::spawn(async move {
                    let _: Result<serde_json::Value, _> = listener
                        .run_mobile_plugin_async(
                            "registerListener",
                            serde_json::json!({"event":"transferAction","handler":channel}),
                        )
                        .await;
                });
                app.manage(Mobile(plugin));
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let _ = (app, api);
            Ok(())
        })
        .build()
}
pub async fn call(
    app: &AppHandle,
    operation: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let plugin = app.state::<Mobile<tauri::Wry>>().0.clone();
        tokio::time::timeout(
            std::time::Duration::from_secs(if operation == "export" || operation == "save" {
                1800
            } else if operation == "chooseDirectory"
                || operation == "scan"
                || operation == "scanImage"
                || operation == "transfer"
                || operation == "saveLogs"
                || operation == "resolveSaved"
            {
                300
            } else if operation == "connectWifi" || operation == "searchRecorderWifi" {
                60
            } else {
                30
            }),
            plugin.run_mobile_plugin_async(
                "execute",
                serde_json::json!({"operation":operation,"payload":payload}),
            ),
        )
        .await
        .map_err(|_| "手机媒体操作超时".to_string())?
        .map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = (app, operation, payload);
        Err("此操作仅用于手机客户端".into())
    }
}

#[tauri::command]
pub async fn native_media(
    app: AppHandle,
    operation: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if ![
        "open",
        "bounds",
        "control",
        "status",
        "close",
        "scan",
        "scanImage",
        "connectWifi",
        "searchRecorderWifi",
        "openWifi",
        "cancelExport",
    ]
    .contains(&operation.as_str())
    {
        return Err("不支持的媒体操作".into());
    }
    let payload = if operation == "searchRecorderWifi" {
        if !payload.is_object() {
            return Err("Wi-Fi 参数格式错误".into());
        }
        let mut p = payload;
        p["search"] = serde_json::json!(true);
        p
    } else {
        payload
    };
    call(&app, &operation, payload).await
}
