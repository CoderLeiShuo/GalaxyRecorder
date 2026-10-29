use crate::{
    connection::client,
    protocol::{authority, video_path, Video},
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, Method, Response, StatusCode},
    routing::get,
    Router,
};
use futures_util::StreamExt;
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    sync::{Mutex, RwLock},
};
use tokio_util::io::ReaderStream;

#[derive(Clone)]
enum Target {
    Remote(String),
    Local(PathBuf),
    Live(String),
}
struct Server {
    port: u16,
    targets: Arc<RwLock<HashMap<String, Target>>>,
}
#[derive(Default)]
pub struct MediaServer(Mutex<Option<Server>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub url: String,
    pub native_url: String,
    pub live: bool,
    pub size: Option<u64>,
}

pub fn remote_url(address: &str, video: &Video, thumbnail: bool) -> Result<String, String> {
    let mut url = reqwest::Url::parse(&format!("http://{}", authority(address)?))
        .map_err(|e| e.to_string())?;
    let mut path = video_path(video)?;
    if thumbnail && video.adapter.as_deref() == Some("L6") {
        let dot = path.rfind('.').ok_or("录像扩展名缺失")?;
        path = format!("{}_T.jpg", &path[..dot]);
    }
    url.set_path(&format!(
        "{}{path}",
        if thumbnail && video.adapter.as_deref() != Some("L6") {
            "/thumb"
        } else {
            ""
        }
    ));
    Ok(url.into())
}

pub async fn remote_size(url: &str) -> Result<u64, String> {
    let response = client()?
        .get(url)
        .header("Range", "bytes=0-0")
        .header("Accept-Encoding", "identity")
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let size = match response.status().as_u16() {
        206 => response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse().ok()),
        200 => response.content_length(),
        _ => None,
    };
    size.filter(|n| *n > 0).ok_or("设备没有返回录像大小".into())
}

impl MediaServer {
    #[cfg(target_os = "ios")]
    pub async fn photo(&self, app: &tauri::AppHandle, video: &Video) -> Result<Source, String> {
        let value = crate::mobile::call(
            app,
            "resolveSaved",
            serde_json::json!({"path":video.saved_path}),
        )
        .await?;
        let path = PathBuf::from(value["path"].as_str().ok_or("无法读取相册录像")?);
        Ok(Source {
            size: tokio::fs::metadata(&path).await.ok().map(|m| m.len()),
            url: self.register(Target::Local(path.clone())).await?,
            native_url: path.to_string_lossy().into(),
            live: false,
        })
    }
    async fn register(&self, target: Target) -> Result<String, String> {
        let mut guard = self.0.lock().await;
        if guard.is_none() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|e| e.to_string())?;
            let port = listener.local_addr().map_err(|e| e.to_string())?.port();
            let targets = Arc::new(RwLock::new(HashMap::new()));
            let router = Router::new()
                .route("/media/{token}", get(serve))
                .with_state(targets.clone());
            tauri::async_runtime::spawn(async move {
                let _ = axum::serve(listener, router).await;
            });
            *guard = Some(Server { port, targets });
        }
        let server = guard.as_ref().unwrap();
        let token = uuid::Uuid::new_v4().simple().to_string();
        server.targets.write().await.insert(token.clone(), target);
        Ok(format!("http://127.0.0.1:{}/media/{token}", server.port))
    }
    pub async fn video(
        &self,
        address: Option<&str>,
        video: &Video,
        app: &tauri::AppHandle,
    ) -> Result<Source, String> {
        let local_root = crate::media_directory(app)?;
        #[cfg(target_os = "ios")]
        if crate::library::external(video)
            && video
                .saved_path
                .as_deref()
                .is_some_and(|path| path.starts_with("ph://"))
        {
            return self.photo(app, video).await;
        }
        if let Some(path) = &video.saved_path {
            #[cfg(target_os = "android")]
            if crate::library::external(video) {
                return Ok(Source {
                    url: path.clone(),
                    native_url: path.clone(),
                    live: false,
                    size: Some(video.size),
                });
            }
            let path = tokio::fs::canonicalize(path)
                .await
                .map_err(|_| "本地录像不存在，请重新下载")?;
            let root = tokio::fs::canonicalize(local_root)
                .await
                .map_err(|e| e.to_string())?;
            if !crate::library::external(video) && path.parent() != Some(root.as_path()) {
                return Err("不能读取媒体库之外的文件".into());
            }
            let url = self.register(Target::Local(path.clone())).await?;
            return Ok(Source {
                url,
                native_url: path.to_string_lossy().into(),
                live: false,
                size: tokio::fs::metadata(&path).await.ok().map(|m| m.len()),
            });
        }
        let remote = remote_url(address.ok_or("请先连接记录仪")?, video, false)?;
        let size = if video.size > 0 {
            Some(video.size)
        } else {
            remote_size(&remote).await.ok()
        };
        Ok(Source {
            size,
            url: self.register(Target::Remote(remote.clone())).await?,
            native_url: remote,
            live: false,
        })
    }
    pub async fn live(&self, rtsp: String) -> Result<Source, String> {
        Ok(Source {
            url: self.register(Target::Live(rtsp.clone())).await?,
            native_url: rtsp,
            live: true,
            size: None,
        })
    }
}

fn response(status: StatusCode, body: Body) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Length,Content-Range,Accept-Ranges",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .unwrap()
}

async fn serve(
    State(targets): State<Arc<RwLock<HashMap<String, Target>>>>,
    Path(token): Path<String>,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    let target = targets.read().await.get(&token).cloned();
    let result = match target {
        Some(Target::Remote(url)) => serve_remote(url, method, headers).await,
        Some(Target::Local(path)) => serve_local(path, method, headers).await,
        Some(Target::Live(url)) => serve_live(url).await,
        None => return response(StatusCode::NOT_FOUND, Body::empty()),
    };
    result.unwrap_or_else(|_| {
        response(
            StatusCode::BAD_GATEWAY,
            Body::from("录像读取失败，请检查连接"),
        )
    })
}

async fn serve_remote(
    url: String,
    method: Method,
    headers: HeaderMap,
) -> Result<Response<Body>, String> {
    let mut request = client()?
        .request(method.clone(), url)
        .timeout(Duration::from_secs(1800))
        .header("Accept-Encoding", "identity");
    if let Some(range) = headers.get(header::RANGE) {
        request = request.header("Range", range);
    }
    let upstream = request.send().await.map_err(|e| e.to_string())?;
    let status = upstream.status();
    if !matches!(status.as_u16(), 200 | 206 | 416) {
        return Err(format!("HTTP {status}"));
    }
    let mut result = response(status, Body::empty());
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            result.headers_mut().insert(name, value.clone());
        }
    }
    if !result.headers().contains_key(header::CONTENT_TYPE) {
        result
            .headers_mut()
            .insert(header::CONTENT_TYPE, "video/mp4".parse().unwrap());
    }
    if method != Method::HEAD {
        *result.body_mut() = Body::from_stream(upstream.bytes_stream());
    }
    Ok(result)
}

pub fn byte_range(value: Option<&str>, length: u64) -> Result<(u64, u64, bool), String> {
    let Some(value) = value else {
        return Ok((0, length, false));
    };
    let (start, end) = value
        .strip_prefix("bytes=")
        .and_then(|r| r.split_once('-'))
        .ok_or("无效的字节范围")?;
    if length == 0 {
        return Err("空文件没有可读取范围".into());
    }
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| "无效范围")?;
        if suffix == 0 {
            return Err("无效范围".into());
        }
        (length.saturating_sub(suffix), length - 1)
    } else {
        let start = start.parse::<u64>().map_err(|_| "无效范围")?;
        let end = if end.is_empty() {
            length - 1
        } else {
            end.parse::<u64>().map_err(|_| "无效范围")?.min(length - 1)
        };
        (start, end)
    };
    if start >= length || end < start {
        return Err("范围超出文件".into());
    }
    Ok((start, end - start + 1, true))
}

async fn serve_local(
    path: PathBuf,
    method: Method,
    headers: HeaderMap,
) -> Result<Response<Body>, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let total = file.metadata().await.map_err(|e| e.to_string())?.len();
    let (start, length, partial) = match byte_range(
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
        total,
    ) {
        Ok(range) => range,
        Err(_) => {
            let mut r = response(StatusCode::RANGE_NOT_SATISFIABLE, Body::empty());
            r.headers_mut().insert(
                header::CONTENT_RANGE,
                format!("bytes */{total}").parse().unwrap(),
            );
            return Ok(r);
        }
    };
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|e| e.to_string())?;
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(ReaderStream::new(file.take(length)))
    };
    let mut result = response(
        if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        },
        body,
    );
    result
        .headers_mut()
        .insert(header::CONTENT_TYPE, "video/mp4".parse().unwrap());
    result
        .headers_mut()
        .insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    result
        .headers_mut()
        .insert(header::CONTENT_LENGTH, length.to_string().parse().unwrap());
    if partial {
        result.headers_mut().insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{total}", start + length - 1)
                .parse()
                .unwrap(),
        );
    }
    Ok(result)
}

pub fn ffmpeg() -> PathBuf {
    ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

async fn serve_live(url: String) -> Result<Response<Body>, String> {
    let mut child = tokio::process::Command::new(ffmpeg())
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-rtsp_transport",
            "tcp",
            "-i",
            &url,
            "-an",
            "-vf",
            "fps=12",
            "-f",
            "mpjpeg",
            "-boundary_tag",
            "frame",
            "pipe:1",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "实时画面需要 FFmpeg")?;
    let reader = ReaderStream::new(child.stdout.take().ok_or("无法读取实时流")?);
    let stream = futures_util::stream::unfold((child, reader), |(child, mut reader)| async move {
        reader.next().await.map(|chunk| (chunk, (child, reader)))
    });
    let mut r = response(StatusCode::OK, Body::from_stream(stream));
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        "multipart/x-mixed-replace; boundary=frame".parse().unwrap(),
    );
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_support_seeking_suffixes_and_reject_invalid_requests() {
        assert_eq!(byte_range(Some("bytes=5-9"), 100).unwrap(), (5, 5, true));
        assert_eq!(byte_range(Some("bytes=-20"), 100).unwrap(), (80, 20, true));
        assert_eq!(byte_range(Some("bytes=80-"), 100).unwrap(), (80, 20, true));
        for range in ["bytes=100-", "bytes=-0", "bytes=9-2", "bytes=1-2,3-4"] {
            assert!(byte_range(Some(range), 100).is_err());
        }
    }
    #[tokio::test]
    async fn local_proxy_streams_ranges_and_rejects_unknown_capabilities() {
        let file =
            std::env::temp_dir().join(format!("galaxy-media-test-{}.mp4", uuid::Uuid::new_v4()));
        tokio::fs::write(&file, b"0123456789").await.unwrap();
        let server = MediaServer::default();
        let url = server.register(Target::Local(file.clone())).await.unwrap();
        assert_eq!(remote_size(&url).await.unwrap(), 10);
        let http = client().unwrap();
        let response = http
            .get(&url)
            .header("Range", "bytes=-3")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 206);
        assert_eq!(response.headers()["content-range"], "bytes 7-9/10");
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"789");
        let response = http
            .get(&url)
            .header("Range", "bytes=12-")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 416);
        let url = url.rsplit_once('/').unwrap().0.to_string() + "/unknown";
        assert_eq!(http.get(url).send().await.unwrap().status().as_u16(), 404);
        tokio::fs::remove_file(file).await.unwrap();
    }
}
