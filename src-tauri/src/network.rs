use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiLink {
    pub id: String,
    pub interface_name: String,
    pub ip: String,
    pub prefix_length: u8,
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    pub platform: String,
    pub available: bool,
    pub selected: Option<WifiLink>,
    pub networks: Vec<WifiLink>,
    pub bound: bool,
    pub default_transport: String,
    pub message: String,
}

#[cfg(target_os = "android")]
struct MobileNetwork<R: Runtime>(tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("recorder-network")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(MobileNetwork(api.register_android_plugin(
                "com.leishuo.galaxyrecorder",
                "RecorderNetworkPlugin",
            )?));
            #[cfg(not(target_os = "android"))]
            let _ = (app, api);
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
async fn mobile_call<T: serde::de::DeserializeOwned>(
    app: &AppHandle,
    command: &str,
    payload: serde_json::Value,
) -> Result<T, String> {
    let plugin = app.state::<MobileNetwork<tauri::Wry>>().0.clone();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        plugin.run_mobile_plugin_async(command, payload),
    )
    .await
    .map_err(|_| "手机 Wi-Fi 网络接口响应超时，请重新打开应用".to_string())?
    .map_err(|error| format!("手机 Wi-Fi 网络接口失败：{error}"))
}

pub async fn inspect(app: &AppHandle) -> Result<NetworkInfo, String> {
    #[cfg(target_os = "android")]
    {
        mobile_call(app, "inspect", serde_json::json!({})).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        desktop_info()
    }
}

pub async fn prepare(app: &AppHandle, multicast: bool) -> Result<NetworkInfo, String> {
    #[cfg(target_os = "android")]
    let info: NetworkInfo =
        mobile_call(app, "prepare", serde_json::json!({"multicast":multicast})).await?;
    #[cfg(not(target_os = "android"))]
    let info = {
        let _ = (app, multicast);
        desktop_info()?
    };
    if !info.available {
        return Err(info.message);
    }
    #[cfg(target_os = "android")]
    if !info.bound {
        return Err("无法将记录仪请求切换到 Wi-Fi，请重新连接车机热点".into());
    }
    Ok(info)
}

pub async fn end_discovery(app: &AppHandle) {
    #[cfg(target_os = "android")]
    {
        let _: Result<serde_json::Value, _> =
            mobile_call(app, "endDiscovery", serde_json::json!({})).await;
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

pub async fn release(app: &AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let _: serde_json::Value = mobile_call(app, "release", serde_json::json!({})).await?;
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn desktop_info() -> Result<NetworkInfo, String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .connect("192.168.43.1:9")
        .map_err(|_| "请先连接车机热点".to_string())?;
    let ip = socket
        .local_addr()
        .map_err(|e| e.to_string())?
        .ip()
        .to_string();
    let available = ip.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_private());
    let selected = available.then(|| WifiLink {
        id: "system-route".into(),
        interface_name: "system".into(),
        ip,
        prefix_length: 24,
        gateway: None,
    });
    Ok(NetworkInfo {
        platform: "desktop".into(),
        available,
        networks: selected.clone().into_iter().collect(),
        selected,
        bound: false,
        default_transport: "system".into(),
        message: if available {
            "将使用电脑当前局域网连接".into()
        } else {
            "请先连接车机热点".into()
        },
    })
}
