use crate::{
    network::{self, NetworkInfo},
    protocol::{a7_probe_accepted, authority, heartbeat_address, same_subnet, subnet_hosts},
};
use futures_util::{stream, StreamExt};
use reqwest::{Client, Url};
use serde::Serialize;
use socket2::{Domain, Protocol, Socket, Type};
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
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
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
        if let Ok(mut trace) = self.trace.lock() {
            let event = DiagnosticEvent {
                elapsed_ms: trace.started.elapsed().as_millis(),
                stage: stage.into(),
                message: message.into(),
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
                let _ = app.emit("connection-progress", serde_json::json!({"checked":checked,"total":total,"message":format!("正在查找记录仪，已检查 {checked}/{total} 个地址…")}));
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
        .user_agent("GalaxyRecorder/0.1.2")
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

async fn response_text(mut response: reqwest::Response, limit: usize) -> Result<String, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > limit {
            return Err("设备响应超过读取上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub async fn text(client: &Client, url: Url, seconds: u64) -> Result<String, String> {
    let response = client
        .get(url)
        .header("Accept-Encoding", "identity")
        .timeout(Duration::from_secs(seconds))
        .send()
        .await
        .map_err(|e| format!("设备未响应：{e}"))?
        .error_for_status()
        .map_err(|e| format!("设备返回错误：{e}"))?;
    response_text(response, 8 * 1024 * 1024).await
}

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
    if !a7_probe_accepted(&body) {
        return Err(format!(
            "HTTP 200，但不是兼容的记录仪响应（{} 字节）",
            body.len()
        ));
    }
    Ok(body)
}

async fn probe_with(
    http: &Client,
    address: &str,
    timeout: Duration,
    source: &str,
) -> Result<Recorder, String> {
    let address = authority(address)?;
    let mut reasons = Vec::new();
    for property in ["DVR", "Camera.Menu.FWversion"] {
        match probe_response(http, endpoint(&address, "get", property)?, timeout).await {
            Ok(body) => {
                let firmware = body
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once('=')?;
                        key.to_ascii_lowercase()
                            .ends_with("fwversion")
                            .then(|| value.trim().to_string())
                    })
                    .unwrap_or_else(|| "待进一步读取".into());
                return Ok(Recorder {
                    address,
                    protocol: "A7 / Config.cgi".into(),
                    firmware,
                    vehicle: "2026 款星越 L".into(),
                    discovered_via: source.into(),
                });
            }
            Err(error) => reasons.push(format!("{property}: {error}")),
        }
    }
    Err(reasons.join("；"))
}

pub async fn connect(
    app: &AppHandle,
    state: &ConnectionState,
    address: String,
) -> Result<Recorder, String> {
    let _operation = state.operation.lock().await;
    state.begin("manual");
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
        state.network(info);
        state.log("http", format!("手动识别 {address}"));
        probe_with(&client()?, &address, Duration::from_secs(5), "manual")
            .await
            .map_err(|error| format!("{address} 连接未完成：{error}"))
    }
    .await;
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
    http: &Client,
    state: &ConnectionState,
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
        if let Ok(recorder) = probe_with(
            http,
            &address.to_string(),
            Duration::from_millis(900),
            &format!("UDP {port}"),
        )
        .await
        {
            return recorder;
        }
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
    let result=async {
        let info=network::prepare(app,true).await?;
        let selected=info.selected.as_ref().ok_or("当前 Wi-Fi 没有可用地址")?;
        state.log("network",format!("使用 Wi-Fi {} /{}，绑定={}，默认网络={}",selected.ip,selected.prefix_length,info.bound,info.default_transport));
        state.network(info.clone());
        let addresses=candidates(&info,preferred.as_deref(),subnet.as_deref())?;
        let total=addresses.len();
        let http=client()?;
        let scan=async {
            let mut requests=stream::iter(addresses).map(|address| {
                let http=&http;
                let preferred=&preferred;
                async move {
                    let timeout=if preferred.as_ref().is_some_and(|value|authority(value).ok().as_deref()==Some(address.as_str())) {1800} else {450};
                    let result=probe_with(http,&address,Duration::from_millis(timeout),"HTTP scan").await;
                    state.progress(app,total);
                    if let Err(error)=&result {state.sample_failure(&address,error);}
                    result.ok()
                }
            }).buffer_unordered(32);
            while let Some(result)=requests.next().await {if result.is_some(){return result;}}
            None
        };
        let first_udp=udp_discover(49142,&info,&http,state);
        let second_udp=udp_discover(53296,&info,&http,state);
        let cancelled=async {loop {if state.cancelled(generation){break;}tokio::time::sleep(Duration::from_millis(100)).await;}};
        tokio::pin!(scan,first_udp,second_udp,cancelled);
        let mut http_done=false;
        let deadline=tokio::time::sleep(Duration::from_secs(35));tokio::pin!(deadline);
        loop {
            tokio::select! {
                result=&mut scan, if !http_done => {if let Some(recorder)=result{return Ok(recorder);} http_done=true;state.log("http","当前网段已检查完毕，继续等待设备广播");}
                recorder=&mut first_udp=>return Ok(recorder),
                recorder=&mut second_udp=>return Ok(recorder),
                _=&mut cancelled=>return Err("查找已取消".into()),
                _=&mut deadline=>return Err(format!("未找到记录仪，已检查当前 Wi-Fi 的 {total} 个地址。请确认连接的是车机热点，并打开一次车机记录仪应用后重试。")),
            }
        }
    }.await;
    network::end_discovery(app).await;
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
    fn candidates_keep_custom_ports_and_cover_real_mask() {
        let link = WifiLink {
            id: "test".into(),
            interface_name: "wlan0".into(),
            ip: "10.205.20.149".into(),
            prefix_length: 23,
            gateway: Some("10.205.20.1".into()),
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
