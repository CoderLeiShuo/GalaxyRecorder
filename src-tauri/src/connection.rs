use crate::{
    network::{self, NetworkInfo},
    protocol::{authority, heartbeat_address, same_subnet, subnet_hosts},
};
use futures_util::{stream, StreamExt};
use reqwest::{Client, Url};
use serde::Serialize;
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::{BTreeMap, HashMap};
use std::{
    collections::HashSet,
    net::{Ipv4Addr, SocketAddrV4},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::net::UdpSocket;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recorder {
    pub address: String,
    pub protocol: String,
    pub firmware: String,
    pub vehicle: String,
    pub discovered_via: String,
    pub network_id: Option<String>,
    pub profile: String,
    pub device_key: String,
    pub vehicle_info: Option<crate::vehicle::VehicleInfo>,
    #[serde(skip)]
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    pub elapsed_ms: u128,
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub method: String,
    pub result: String,
    pub network: Option<NetworkInfo>,
    pub checked: usize,
    pub total: usize,
    pub last_error: Option<String>,
    pub events: Vec<DiagnosticEvent>,
}

struct Trace {
    started: Instant,
    snapshot: Diagnostics,
    sampled_failures: usize,
}

pub struct ConnectionState {
    trace: Mutex<Trace>,
    generation: AtomicU64,
    pub operation: tokio::sync::Mutex<()>,
    selected_profile: Mutex<String>,
    devices: Mutex<HashMap<String, Recorder>>,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            selected_profile: Mutex::new("AUTO".into()),
            devices: Mutex::new(HashMap::new()),
            operation: tokio::sync::Mutex::new(()),
            trace: Mutex::new(Trace {
                started: Instant::now(),
                sampled_failures: 0,
                snapshot: Diagnostics {
                    method: "none".into(),
                    result: "idle".into(),
                    network: None,
                    checked: 0,
                    total: 0,
                    last_error: None,
                    events: Vec::new(),
                },
            }),
        }
    }
}

impl ConnectionState {
    pub fn set_profile(&self, profile: Option<&str>) -> Result<(), String> {
        *self.selected_profile.lock().map_err(|_| "连接配置不可用")? =
            crate::adapters::profile(profile)?.into();
        Ok(())
    }
    pub fn profile(&self) -> String {
        self.selected_profile
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|_| "AUTO".into())
    }
    pub fn recorder(&self, address: &str) -> Option<Recorder> {
        self.devices.lock().ok()?.get(address).cloned()
    }
    pub fn family(&self, address: &str) -> String {
        self.recorder(address)
            .map(|r| r.profile)
            .unwrap_or_else(|| "A7".into())
    }

    pub fn begin(&self, method: &str) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut trace) = self.trace.lock() {
            trace.started = Instant::now();
            trace.sampled_failures = 0;
            trace.snapshot = Diagnostics {
                method: method.into(),
                result: "running".into(),
                network: None,
                checked: 0,
                total: 0,
                last_error: None,
                events: Vec::new(),
            };
        }
        generation
    }
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    fn cancelled(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) != generation
    }
    pub fn log(&self, stage: &str, message: impl Into<String>) {
        let message = message.into();
        crate::journal::record(stage, &message);
        if let Ok(mut trace) = self.trace.lock() {
            let event = DiagnosticEvent {
                elapsed_ms: trace.started.elapsed().as_millis(),
                stage: stage.into(),
                message,
            };
            if trace.snapshot.events.len() == 80 {
                trace.snapshot.events.remove(0);
            }
            trace.snapshot.events.push(event);
        }
    }
    fn sample_failure(&self, address: &str, message: &str) {
        let should_log = self
            .trace
            .lock()
            .map(|mut trace| {
                trace.sampled_failures += 1;
                trace.sampled_failures <= 6
            })
            .unwrap_or(false);
        if should_log {
            self.log("http", format!("{address}: {message}"));
        }
    }
    pub fn network(&self, network: NetworkInfo) {
        if let Ok(mut trace) = self.trace.lock() {
            trace.snapshot.network = Some(network);
        }
    }
    fn progress(&self, app: &AppHandle, total: usize) {
        if let Ok(mut trace) = self.trace.lock() {
            trace.snapshot.checked += 1;
            trace.snapshot.total = total;
            let checked = trace.snapshot.checked;
            if checked % 16 == 0 || checked == total {
                let _ = app.emit("connection-progress", serde_json::json!({"checked":checked,"total":total,"message":"正在连接记录仪，请稍候…"}));
            }
        }
    }
    pub fn finish(&self, error: Option<&str>) {
        if let Ok(mut trace) = self.trace.lock() {
            trace.snapshot.result = if error.is_some() { "failed" } else { "success" }.into();
            trace.snapshot.last_error = error.map(str::to_owned);
        }
    }
    pub fn snapshot(&self) -> Result<Diagnostics, String> {
        self.trace
            .lock()
            .map(|trace| trace.snapshot.clone())
            .map_err(|_| "诊断信息暂不可读取".into())
    }
}

pub fn client() -> Result<Client, String> {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(4))
        .read_timeout(Duration::from_secs(15))
        .user_agent(concat!("GalaxyRecorder/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

pub fn endpoint(address: &str, action: &str, property: &str) -> Result<Url, String> {
    let mut url = Url::parse(&format!(
        "http://{}/cgi-bin/Config.cgi",
        authority(address)?
    ))
    .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("action", action)
        .append_pair("property", property);
    Ok(url)
}

pub(crate) async fn response_text(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<String, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > limit {
            return Err("设备响应超过读取上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
async fn probe_response(http: &Client, url: Url, timeout: Duration) -> Result<String, String> {
    let response = http
        .get(url)
        .header("Accept-Encoding", "identity")
        .timeout(timeout)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "请求超时".to_string()
            } else if error.is_connect() {
                "地址不可达或端口未开放".to_string()
            } else {
                format!("网络请求失败：{error}")
            }
        })?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let body = response_text(response, 65536).await?;
    if !crate::protocol::a7_probe_accepted(&body) {
        return Err(format!(
            "HTTP 200，但不是兼容的记录仪响应（{} 字节）",
            body.len()
        ));
    }
    Ok(body)
}

async fn probe_selected(
    address: &str,
    profile: &str,
    timeout: Duration,
    source: &str,
) -> Result<Recorder, String> {
    let address = authority(address)?;
    let detection = crate::adapters::probe(&address, profile, timeout).await?;
    let firmware = detection
        .fields
        .iter()
        .find(|(k, _)| {
            k.to_ascii_lowercase().ends_with("fwversion") || k.eq_ignore_ascii_case("version")
        })
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "设备未提供".into());
    let vehicle_info = crate::vehicle::info(&detection.fields);
    Ok(Recorder {
        address,
        protocol: format!(
            "{} / {}",
            detection.family,
            if detection.family == "L6" {
                "Vyou"
            } else {
                "Config.cgi"
            }
        ),
        firmware,
        vehicle: vehicle_info
            .as_ref()
            .map(|v| v.name.clone())
            .unwrap_or_else(|| "我的车辆".into()),
        profile: detection.family,
        device_key: String::new(),
        vehicle_info,
        fields: detection.fields,
        discovered_via: source.into(),
        network_id: None,
    })
}
async fn finalize(
    app: &AppHandle,
    state: &ConnectionState,
    mut recorder: Recorder,
    enrich: bool,
) -> Result<Recorder, String> {
    if enrich && recorder.profile == "L6" {
        crate::adapters::reset_session(&recorder.address).await;
        crate::adapters::l6_session(&recorder.address).await?;
    }
    if enrich && recorder.profile != "L6" {
        for property in ["Camera.Menu.*", "Camera.Menu.FWversion"] {
            if let Ok(url) = endpoint(&recorder.address, "get", property) {
                let response = client()?
                    .get(url)
                    .timeout(Duration::from_secs(2))
                    .send()
                    .await;
                if let Ok(response) = response {
                    if response.status().is_success() {
                        if let Ok(body) = response_text(response, 128 * 1024).await {
                            if crate::adapters::accepted_fields(&body) {
                                recorder.fields.extend(crate::device::fields(&body));
                            }
                        }
                    }
                }
            }
        }
    }
    let network = state
        .snapshot()
        .ok()
        .and_then(|s| s.network)
        .and_then(|n| n.selected)
        .and_then(|n| n.fingerprint);
    recorder.device_key = crate::vehicle::device_key(
        &recorder.address,
        &recorder.profile,
        &recorder.fields,
        network.as_deref(),
    );
    let memory = crate::settings::remember_device(
        app,
        crate::vehicle::DeviceMemory {
            key: recorder.device_key.clone(),
            address: recorder.address.clone(),
            profile: recorder.profile.clone(),
            vehicle: crate::vehicle::info(&recorder.fields),
        },
    )
    .await?;
    recorder.vehicle_info = memory.vehicle;
    recorder.vehicle = recorder
        .vehicle_info
        .as_ref()
        .map(|v| v.name.clone())
        .unwrap_or_else(|| "我的车辆".into());
    if let Some((_, firmware)) = recorder.fields.iter().find(|(k, _)| {
        k.to_ascii_lowercase().ends_with("fwversion") || k.eq_ignore_ascii_case("version")
    }) {
        recorder.firmware = firmware.clone();
    }
    state
        .devices
        .lock()
        .map_err(|_| "连接会话不可用")?
        .insert(recorder.address.clone(), recorder.clone());
    Ok(recorder)
}

pub async fn connect(
    app: &AppHandle,
    state: &ConnectionState,
    address: String,
) -> Result<Recorder, String> {
    let _operation = state.operation.lock().await;
    let generation = state.begin("manual");
    let selected_profile = state.profile();
    let result = async {
        let address = authority(&address)?;
        let info = network::prepare(app, false).await?;
        state.log(
            "network",
            format!(
                "Wi-Fi 已准备，绑定={}，默认网络={}",
                info.bound, info.default_transport
            ),
        );
        let network_id = info.selected.as_ref().map(|link| link.id.clone());
        state.network(info);
        state.log("http", format!("手动识别 {address}"));
        probe_selected(
            &address,
            &selected_profile,
            Duration::from_secs(3),
            "manual",
        )
        .await
        .map(|mut recorder| {
            recorder.network_id = network_id;
            recorder
        })
        .map_err(|error| format!("{address} 连接未完成：{error}"))
    }
    .await;
    let result = match result {
        Ok(recorder) if !state.cancelled(generation) => {
            let value = finalize(app, state, recorder, true).await;
            if state.cancelled(generation) {
                Err("连接已取消".into())
            } else {
                value
            }
        }
        Ok(_) => Err("连接已取消".into()),
        Err(error) => Err(error),
    };
    match &result {
        Ok(recorder) => state.log("connected", format!("已识别 {}", recorder.address)),
        Err(error) => state.log("failed", error),
    }
    state.finish(result.as_ref().err().map(String::as_str));
    result
}

fn candidates(
    info: &NetworkInfo,
    preferred: Option<&str>,
    explicit_subnet: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut addresses = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |value: String| {
        if authority(&value).is_ok() && seen.insert(value.clone()) {
            addresses.push(value);
        }
    };
    if let Some(preferred) = preferred {
        if let Ok(value) = authority(preferred) {
            add(value);
        }
    }
    for address in crate::adapters::FACTORY_ADDRESSES {
        add(address.to_string());
    }
    let selected = info.selected.as_ref().ok_or("当前 Wi-Fi 没有可用地址")?;
    if let Some(gateway) = &selected.gateway {
        add(gateway.clone());
    }
    let local = selected
        .ip
        .parse::<Ipv4Addr>()
        .map_err(|_| "当前 Wi-Fi 地址无效")?;
    let (scan_ip, prefix) = if let Some(subnet) = explicit_subnet {
        let value = authority(&format!("{subnet}.1"))?;
        (value.parse::<Ipv4Addr>().map_err(|_| "网段格式不正确")?, 24)
    } else {
        (local, selected.prefix_length)
    };
    // Search the device's own /24 first, then the other addresses covered by its mask.
    for ip in subnet_hosts(scan_ip, 24) {
        if same_subnet(ip, scan_ip, prefix) {
            add(ip.to_string());
        }
    }
    for ip in subnet_hosts(scan_ip, prefix) {
        add(ip.to_string());
    }
    addresses.retain(|address| address.split(':').next() != Some(local.to_string().as_str()));
    Ok(addresses)
}

#[derive(Serialize)]
pub struct Health {
    connected: bool,
    definitive: bool,
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recorder: Option<Recorder>,
}
fn network_issue(info: &NetworkInfo, expected: Option<&str>) -> Option<String> {
    if !info.available || info.selected.is_none() {
        return Some("车机 Wi-Fi 已断开，请重新连接记录仪".into());
    }
    if info.platform == "android" {
        if expected.is_some_and(|id| info.selected.as_ref().is_none_or(|link| link.id != id)) {
            return Some("Wi-Fi 已切换，原记录仪连接已断开".into());
        }
        if !info.bound {
            return Some("记录仪 Wi-Fi 连接已失效，请重新连接".into());
        }
    }
    None
}
#[tauri::command]
pub async fn check_recorder_connection(
    app: AppHandle,
    state: tauri::State<'_, ConnectionState>,
    address: String,
    network_id: Option<String>,
    profile: Option<String>,
) -> Result<Health, String> {
    let address = authority(&address)?;
    let info = network::inspect(&app).await?;
    if let Some(reason) = network_issue(&info, network_id.as_deref()) {
        state.log("connection-check", &reason);
        return Ok(Health {
            connected: false,
            definitive: true,
            reason: Some(reason),
            recorder: None,
        });
    }
    // A health check never selects or binds a new network. A cold process restores the protocol session on this same network.
    let result = probe_selected(
        &address,
        profile.as_deref().unwrap_or(&state.family(&address)),
        Duration::from_millis(1500),
        "health-check",
    )
    .await;
    Ok(match result {
        Ok(recorder) => {
            let recorder = if let Some(cached) = state.recorder(&address) {
                cached
            } else {
                state.network(info);
                finalize(&app, &state, recorder, true).await?
            };
            Health {
                connected: true,
                definitive: false,
                reason: None,
                recorder: Some(recorder),
            }
        }
        Err(_) => Health {
            connected: false,
            definitive: false,
            reason: Some("记录仪已失联，请确认车机热点和记录仪已开启".into()),
            recorder: None,
        },
    })
}

fn udp_socket(port: u16) -> Result<UdpSocket, String> {
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    socket
        .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())
        .map_err(|e| e.to_string())?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    UdpSocket::from_std(socket.into()).map_err(|e| e.to_string())
}

async fn udp_discover(
    port: u16,
    info: &NetworkInfo,
    _http: &Client,
    state: &ConnectionState,
    selected_profile: &str,
) -> Recorder {
    let socket = match udp_socket(port) {
        Ok(socket) => {
            state.log("udp", format!("已开启设备广播监听 {port}"));
            socket
        }
        Err(error) => {
            state.log(
                "udp",
                format!("广播端口 {port} 不可用：{error}；继续 HTTP 识别"),
            );
            return std::future::pending().await;
        }
    };
    let mut buffer = [0u8; 4096];
    loop {
        let Ok((size, _peer)) = socket.recv_from(&mut buffer).await else {
            continue;
        };
        let Some(address) = heartbeat_address(&String::from_utf8_lossy(&buffer[..size])) else {
            continue;
        };
        let on_wifi = info.networks.iter().any(|link| {
            link.ip
                .parse::<Ipv4Addr>()
                .is_ok_and(|local| same_subnet(address, local, link.prefix_length))
        });
        if !on_wifi {
            continue;
        }
        state.log("udp", format!("收到 {address} 的设备广播，正在确认协议"));
        if let Ok(recorder) = probe_selected(
            &address.to_string(),
            selected_profile,
            Duration::from_millis(900),
            &format!("UDP {port}"),
        )
        .await
        {
            return recorder;
        }
    }
}

async fn l6_discover(
    port: u16,
    info: &NetworkInfo,
    state: &ConnectionState,
    profile: &str,
) -> Recorder {
    if !matches!(crate::adapters::family(profile), "AUTO" | "L6") {
        return std::future::pending().await;
    }
    let local = info
        .selected
        .as_ref()
        .and_then(|n| n.ip.parse::<Ipv4Addr>().ok())
        .unwrap_or(Ipv4Addr::UNSPECIFIED);
    let socket = match udp_socket(port) {
        Ok(socket) => socket,
        Err(_) => return std::future::pending().await,
    };
    let group = Ipv4Addr::new(238, 238, 238, 238);
    let _ = socket.join_multicast_v4(group, local);
    let _ = socket.set_multicast_ttl_v4(1);
    // Replies to the active probe may return to an ephemeral source port.
    let active = match UdpSocket::bind((local, 0)).await {
        Ok(socket) => socket,
        Err(_) => return std::future::pending().await,
    };
    let _ = active.set_broadcast(true);
    let _ = active.set_multicast_ttl_v4(1);
    for target in [group, Ipv4Addr::BROADCAST] {
        let _ = active.send_to(b"hello|vyou", (target, 3702)).await;
    }
    let mut bytes = [0u8; 4096];
    let mut active_bytes = [0u8; 4096];
    loop {
        let peer = tokio::select! {result=socket.recv_from(&mut bytes)=>result,result=active.recv_from(&mut active_bytes)=>result};
        let Ok((_, peer)) = peer else { continue };
        let std::net::IpAddr::V4(ip) = peer.ip() else {
            continue;
        };
        if ip == local
            || !info.networks.iter().any(|n| {
                n.ip.parse()
                    .is_ok_and(|local| same_subnet(ip, local, n.prefix_length))
            })
        {
            continue;
        }
        if let Ok(recorder) = probe_selected(
            &ip.to_string(),
            profile,
            Duration::from_millis(900),
            &format!("Vyou UDP {port}"),
        )
        .await
        {
            return recorder;
        }
        state.log("vyou", "收到设备广播，等待有效协议响应");
    }
}

pub async fn discover(
    app: &AppHandle,
    state: &ConnectionState,
    preferred: Option<String>,
    subnet: Option<String>,
) -> Result<Recorder, String> {
    let _operation = state.operation.lock().await;
    let generation = state.begin("automatic");
    let selected_profile = state.profile();
    let result=async {
        let mut ready=network::prepare(app,true).await;
        for delay in [400,1200] {
            if ready.is_ok(){break;}
            if state.cancelled(generation){return Err("连接已取消".into());}
            tokio::time::sleep(Duration::from_millis(delay)).await;
            ready=network::prepare(app,true).await;
        }
        let info=ready?;
        let selected=info.selected.as_ref().ok_or("当前 Wi-Fi 没有可用地址")?;
        state.log("network",format!("使用 Wi-Fi {} /{}，绑定={}，默认网络={}",selected.ip,selected.prefix_length,info.bound,info.default_transport));
        state.network(info.clone());
        let addresses=candidates(&info,preferred.as_deref(),subnet.as_deref())?;
        let total=addresses.len()*3;
        let http=client()?;
        let scan=async {
            for attempt in 0..3 {
            if attempt>0 {tokio::time::sleep(Duration::from_millis(1200)).await;}
            let mut requests=stream::iter(addresses.clone()).map(|address| {
                let preferred=&preferred;
                let selected_profile=&selected_profile;
                async move {
                    let timeout=if preferred.as_ref().is_some_and(|value|authority(value).ok().as_deref()==Some(address.as_str())) {1800} else {800};
                    let result=probe_selected(&address,&selected_profile,Duration::from_millis(timeout),"HTTP scan").await;
                    state.progress(app,total);
                    if let Err(error)=&result {state.sample_failure(&address,error);}
                    result.ok()
                }
            }).buffer_unordered(32);
            while let Some(result)=requests.next().await {if result.is_some(){return result;}}
            }
            None
        };
        let first_udp=udp_discover(49142,&info,&http,state,&selected_profile);
        let second_udp=udp_discover(53296,&info,&http,state,&selected_profile);
        let cancelled=async {loop {if state.cancelled(generation){break;}tokio::time::sleep(Duration::from_millis(100)).await;}};
        let l6_first=l6_discover(3702,&info,state,&selected_profile);
        let l6_second=l6_discover(3703,&info,state,&selected_profile);
        tokio::pin!(scan,first_udp,second_udp,l6_first,l6_second,cancelled);
        let mut http_done=false;
        let deadline=tokio::time::sleep(Duration::from_secs(35));tokio::pin!(deadline);
        loop {
            tokio::select! {
                result=&mut scan, if !http_done => {if let Some(recorder)=result{return Ok(recorder);} http_done=true;state.log("http","当前网段已检查完毕，继续等待设备广播");}
                recorder=&mut first_udp=>return Ok(recorder),
                recorder=&mut second_udp=>return Ok(recorder),
                recorder=&mut l6_first=>return Ok(recorder),
                recorder=&mut l6_second=>return Ok(recorder),
                _=&mut cancelled=>return Err("查找已取消".into()),
                _=&mut deadline=>return Err("连接未成功，请连接车机热点，并开启车机记录仪后重试。".into()),
            }
        }
    }.await.map(|mut recorder: Recorder| {
        recorder.network_id = state.snapshot().ok().and_then(|trace| trace.network).and_then(|info| info.selected).map(|link| link.id);
        recorder
    });
    network::end_discovery(app).await;
    let result = match result {
        Ok(recorder) if !state.cancelled(generation) => {
            let value = finalize(app, state, recorder, true).await;
            if state.cancelled(generation) {
                Err("连接已取消".into())
            } else {
                value
            }
        }
        Ok(_) => Err("连接已取消".into()),
        Err(error) => Err(error),
    };
    match &result {
        Ok(recorder) => state.log(
            "connected",
            format!(
                "已通过 {} 找到 {}",
                recorder.discovered_via, recorder.address
            ),
        ),
        Err(error) => state.log("failed", error),
    }
    state.finish(result.as_ref().err().map(String::as_str));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::WifiLink;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[test]
    fn health_requires_the_same_wifi_but_not_internet_access() {
        let mut info = NetworkInfo {
            platform: "android".into(),
            available: true,
            bound: true,
            default_transport: "cellular".into(),
            selected: Some(WifiLink {
                id: "car-wifi".into(),
                interface_name: "wlan0".into(),
                ip: "192.168.43.20".into(),
                prefix_length: 24,
                gateway: None,
                fingerprint: None,
            }),
            networks: vec![],
            message: String::new(),
        };
        assert!(network_issue(&info, Some("car-wifi")).is_none());
        assert!(network_issue(&info, Some("different-wifi")).is_some());
        info.bound = false;
        assert!(network_issue(&info, Some("car-wifi")).is_some());
        info.available = false;
        info.selected = None;
        assert!(network_issue(&info, None).is_some());
    }

    #[test]
    fn candidates_keep_custom_ports_and_cover_real_mask() {
        let link = WifiLink {
            id: "test".into(),
            interface_name: "wlan0".into(),
            ip: "10.205.20.149".into(),
            prefix_length: 23,
            gateway: Some("10.205.20.1".into()),
            fingerprint: None,
        };
        let info = NetworkInfo {
            platform: "test".into(),
            available: true,
            selected: Some(link.clone()),
            networks: vec![link],
            bound: true,
            default_transport: "cellular".into(),
            message: String::new(),
        };
        let hosts = candidates(&info, Some("10.205.21.7:8080"), None).unwrap();
        assert_eq!(hosts[0], "10.205.21.7:8080");
        assert!(hosts.contains(&"10.205.21.9".to_string()));
        assert!(!hosts.contains(&"10.205.20.149".to_string()));
        assert!(!hosts.contains(&"10.205.22.1".to_string()));
    }

    async fn server(body: &'static str, status: u16) -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).await;
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        Url::parse(&format!(
            "http://{address}/cgi-bin/Config.cgi?action=get&property=DVR"
        ))
        .unwrap()
    }
    #[tokio::test]
    async fn http_accepts_the_short_reply_that_old_version_rejected() {
        let http = client().unwrap();
        assert_eq!(
            probe_response(
                &http,
                server("0\r\nOK\r\n", 200).await,
                Duration::from_secs(2)
            )
            .await
            .unwrap(),
            "0\r\nOK\r\n"
        );
        assert!(probe_response(
            &http,
            server("<html>OK</html>", 200).await,
            Duration::from_secs(2)
        )
        .await
        .is_err());
        assert!(
            probe_response(&http, server("OK", 404).await, Duration::from_secs(2))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn udp_listener_receives_camera_heartbeat() {
        let socket = udp_socket(0).unwrap();
        let address = socket.local_addr().unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender
            .send_to(
                b"IP=192.168.43.8\r\nrfilename=test.mp4",
                SocketAddrV4::new(Ipv4Addr::LOCALHOST, address.port()),
            )
            .await
            .unwrap();
        let mut buffer = [0u8; 4096];
        let (length, _) =
            tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut buffer))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            heartbeat_address(&String::from_utf8_lossy(&buffer[..length])),
            Some(Ipv4Addr::new(192, 168, 43, 8))
        );
    }
}
