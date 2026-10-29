//! Persistent HTTP byte-range transfers. The on-disk length is the resume cursor.
use reqwest::{header, Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default, Serialize, Deserialize)]
struct Checkpoint {
    resource: String,
    validator: Option<String>,
    total: Option<u64>,
}
fn checkpoint(path: &Path) -> PathBuf {
    path.with_extension("resume")
}
fn validator(response: &Response) -> Option<String> {
    response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.starts_with("W/"))
        .map(str::to_owned)
        .or_else(|| {
            response
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        })
}
fn content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (span, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = span.split_once('-')?;
    let (start, end, total) = (start.parse().ok()?, end.parse().ok()?, total.parse().ok()?);
    (start <= end && end < total).then_some((start, end, total))
}
async fn cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
}
async fn send(
    client: &Client,
    url: &str,
    range: Option<(u64, Option<u64>)>,
    tag: Option<&str>,
    cancel: &AtomicBool,
) -> Result<Response, String> {
    let mut request = client
        .get(url)
        .header(header::ACCEPT_ENCODING, "identity")
        .timeout(Duration::from_secs(1800));
    if let Some((start, end)) = range {
        request = request.header(
            header::RANGE,
            format!(
                "bytes={start}-{}",
                end.map(|v| v.to_string()).unwrap_or_default()
            ),
        );
    }
    if let Some(tag) = tag {
        request = request.header(header::IF_RANGE, tag);
    }
    tokio::select! { result=request.send()=>result.map_err(|e|e.to_string()), _=cancelled(cancel)=>Err("下载已暂停，进度已保留".into()) }
}
async fn prefix_matches(
    client: &Client,
    url: &str,
    path: &Path,
    length: u64,
    total: Option<u64>,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    // Devices without validators, and address changes, must still prove this is the same recording.
    let count = length.min(64 * 1024);
    let mut response = send(client, url, Some((0, Some(count - 1))), None, cancel).await?;
    if !response.status().is_success() {
        return Ok(false);
    }
    let remote_total = if response.status() == StatusCode::PARTIAL_CONTENT {
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(content_range)
            .map(|r| r.2)
    } else {
        response.content_length()
    };
    if total.zip(remote_total).is_some_and(|(a, b)| a != b) {
        return Ok(false);
    }
    let mut remote = Vec::new();
    while remote.len() < count as usize {
        let chunk = tokio::select! { value=response.chunk()=>value.map_err(|e|e.to_string())?, _=cancelled(cancel)=>return Err("下载已暂停，进度已保留".into()) };
        let Some(chunk) = chunk else {
            break;
        };
        remote.extend_from_slice(&chunk[..chunk.len().min(count as usize - remote.len())]);
    }
    let mut local = vec![0; count as usize];
    tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?
        .read_exact(&mut local)
        .await
        .map_err(|e| e.to_string())?;
    Ok(remote == local)
}
pub async fn download(
    client: &Client,
    url: &str,
    path: &Path,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64, String> {
    let resource = reqwest::Url::parse(url)
        .map_err(|e| e.to_string())?
        .path()
        .to_owned();
    let old: Checkpoint = tokio::fs::read(checkpoint(path))
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut offset = tokio::fs::metadata(path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if offset > 0
        && (old.resource != resource
            || old.total.is_some_and(|n| offset > n)
            || !prefix_matches(client, url, path, offset, old.total, &cancel).await?)
    {
        offset = 0;
    }
    let mut response = send(
        client,
        url,
        (offset > 0).then_some((offset, None)),
        old.validator.as_deref().filter(|_| offset > 0),
        &cancel,
    )
    .await?;
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        let total = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("bytes */"))
            .and_then(|s| s.parse::<u64>().ok());
        if offset > 0 && total == Some(offset) && old.total == total {
            progress(offset, total);
            return Ok(offset);
        }
        offset = 0;
        response = send(client, url, None, None, &cancel).await?;
    }
    if response.status() == StatusCode::OK {
        offset = 0;
    }
    let total = if response.status() == StatusCode::PARTIAL_CONTENT {
        let (start, end, total) = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(content_range)
            .ok_or("设备返回了无效的续传范围")?;
        if start != offset
            || end + 1 != total
            || response
                .content_length()
                .is_some_and(|n| n != end - start + 1)
        {
            return Err("设备返回的续传位置不一致，已保留原进度".into());
        }
        if offset > 0
            && (old.total.is_some_and(|n| n != total)
                || old
                    .validator
                    .as_ref()
                    .zip(validator(&response).as_ref())
                    .is_some_and(|(a, b)| a != b))
        {
            return Err("录像内容已改变，请删除任务后重新下载".into());
        }
        Some(total)
    } else if response.status() == StatusCode::OK {
        response.content_length()
    } else {
        return Err(format!("录像读取失败 HTTP {}", response.status()));
    };
    if response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|s| s.contains("text/") || s.contains("json") || s.contains("xml"))
    {
        return Err("设备返回了文本内容，未保存为录像".into());
    }
    let state = Checkpoint {
        resource,
        validator: validator(&response),
        total,
    };
    let temporary = path.with_extension("resume.tmp");
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(path)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::write(
        &temporary,
        serde_json::to_vec(&state).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;
    tokio::fs::rename(&temporary, checkpoint(path))
        .await
        .map_err(|e| e.to_string())?;
    let mut received = offset;
    progress(received, total);
    let result = async {
        loop {
            let chunk = tokio::select! { value=response.chunk()=>value.map_err(|e|e.to_string())?, _=cancelled(&cancel)=>return Err("下载已暂停，进度已保留".to_string()) };
            let Some(chunk)=chunk else {break;};
            received=received.checked_add(chunk.len() as u64).ok_or("文件过大")?;
            if total.is_some_and(|n|received>n) { return Err("响应长度超过录像大小".into()); }
            file.write_all(&chunk).await.map_err(|e|e.to_string())?; progress(received,total);
        }
        if cancel.load(Ordering::Relaxed) { return Err("下载已暂停，进度已保留".into()); }
        if received==0 || total.is_some_and(|n|n!=received) { return Err("录像尚未下载完整，可继续下载".into()); }
        Ok(received)
    }.await;
    file.flush().await.map_err(|e| e.to_string())?;
    file.sync_all().await.map_err(|e| e.to_string())?;
    result
}
pub async fn remove_checkpoint(path: &Path) {
    let _ = tokio::fs::remove_file(checkpoint(path)).await;
    let _ = tokio::fs::remove_file(path.with_extension("resume.tmp")).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{HeaderMap, StatusCode as Code},
        response::IntoResponse,
        routing::get,
        Router,
    };
    async fn server(ignore: bool, bad: bool) -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new().route(
            "/v.mp4",
            get(move |headers: HeaderMap| async move {
                let data = b"abcdefghijklmnop";
                let range = headers
                    .get("range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("bytes="))
                    .and_then(|v| v.split_once('-'));
                let start = range.and_then(|r| r.0.parse::<usize>().ok()).unwrap_or(0);
                let end = range
                    .and_then(|r| r.1.parse::<usize>().ok())
                    .unwrap_or(data.len() - 1)
                    .min(data.len() - 1);
                if !ignore && range.is_some() {
                    if start >= data.len() {
                        return (
                            Code::RANGE_NOT_SATISFIABLE,
                            [("content-range", "bytes */16".to_string())],
                            Vec::new(),
                        )
                            .into_response();
                    }
                    return (
                        Code::PARTIAL_CONTENT,
                        [
                            (
                                "content-range",
                                format!(
                                    "bytes {}-{end}/16",
                                    if bad && start > 0 { 0 } else { start }
                                ),
                            ),
                            ("etag", "\"v1\"".to_string()),
                        ],
                        data[start..=end].to_vec(),
                    )
                        .into_response();
                }
                (Code::OK, [("etag", "\"v1\"".to_string())], data.to_vec()).into_response()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v.mp4", listener.local_addr().unwrap());
        let task = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
        (url, task)
    }
    async fn fixture(prefix: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("galaxy-{}.part", uuid::Uuid::new_v4()));
        tokio::fs::write(&path, prefix).await.unwrap();
        tokio::fs::write(
            checkpoint(&path),
            serde_json::to_vec(&Checkpoint {
                resource: "/v.mp4".into(),
                validator: Some("\"v1\"".into()),
                total: Some(16),
            })
            .unwrap(),
        )
        .await
        .unwrap();
        path
    }
    #[tokio::test]
    async fn resumes_existing_bytes_after_restart_without_duplication() {
        let (url, server) = server(false, false).await;
        let path = fixture(b"abcdef").await;
        let mut initial = 0;
        let mut first = true;
        let size = download(
            &Client::new(),
            &url,
            &path,
            Arc::new(AtomicBool::new(false)),
            |n, _| {
                if first {
                    initial = n;
                    first = false;
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(initial, 6);
        assert_eq!(size, 16);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abcdefghijklmnop");
        remove_checkpoint(&path).await;
        tokio::fs::remove_file(path).await.unwrap();
        server.abort();
    }
    #[tokio::test]
    async fn ignored_range_and_changed_recording_restart_safely() {
        for (ignore, prefix) in [(true, b"abcdef".as_slice()), (false, b"XXXXXX".as_slice())] {
            let (url, server) = server(ignore, false).await;
            let path = fixture(prefix).await;
            download(
                &Client::new(),
                &url,
                &path,
                Arc::new(AtomicBool::new(false)),
                |_, _| {},
            )
            .await
            .unwrap();
            assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abcdefghijklmnop");
            remove_checkpoint(&path).await;
            tokio::fs::remove_file(path).await.unwrap();
            server.abort();
        }
    }
    #[tokio::test]
    async fn rejects_wrong_resume_offset_preserving_partial() {
        let (url, server) = server(false, true).await;
        let path = fixture(b"abcdef").await;
        assert!(download(
            &Client::new(),
            &url,
            &path,
            Arc::new(AtomicBool::new(false)),
            |_, _| {}
        )
        .await
        .is_err());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abcdef");
        remove_checkpoint(&path).await;
        tokio::fs::remove_file(path).await.unwrap();
        server.abort();
    }
    #[tokio::test]
    async fn completed_partial_handles_416() {
        let (url, server) = server(false, false).await;
        let path = fixture(b"abcdefghijklmnop").await;
        assert_eq!(
            download(
                &Client::new(),
                &url,
                &path,
                Arc::new(AtomicBool::new(false)),
                |_, _| {}
            )
            .await
            .unwrap(),
            16
        );
        remove_checkpoint(&path).await;
        tokio::fs::remove_file(path).await.unwrap();
        server.abort();
    }
    #[tokio::test]
    async fn stopping_mid_stream_keeps_bytes_and_resumes_after_reopening() {
        let app = Router::new().route(
            "/v.mp4",
            get(|headers: HeaderMap| async move {
                let data = b"abcdefghijklmnop";
                if let Some((start, end)) = headers
                    .get("range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("bytes="))
                    .and_then(|v| v.split_once('-'))
                {
                    let start: usize = start.parse().unwrap();
                    let end: usize = end.parse().unwrap_or(15);
                    return (
                        Code::PARTIAL_CONTENT,
                        [
                            ("content-range", format!("bytes {start}-{end}/16")),
                            ("etag", "\"v1\"".into()),
                        ],
                        data[start..=end].to_vec(),
                    )
                        .into_response();
                }
                let chunks = futures_util::stream::unfold(0usize, |offset| async move {
                    if offset >= 16 {
                        return None;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    Some((
                        Ok::<_, std::io::Error>(b"abcdefghijklmnop"[offset..offset + 2].to_vec()),
                        offset + 2,
                    ))
                });
                axum::http::Response::builder()
                    .header("content-length", "16")
                    .header("etag", "\"v1\"")
                    .body(axum::body::Body::from_stream(chunks))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v.mp4", listener.local_addr().unwrap());
        let server = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
        let path = fixture(b"").await;
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        assert!(download(&Client::new(), &url, &path, cancel, move |n, _| {
            if n >= 6 {
                flag.store(true, Ordering::Relaxed);
            }
        })
        .await
        .is_err());
        let retained = tokio::fs::metadata(&path).await.unwrap().len();
        assert!(retained >= 6 && retained < 16);
        let mut initial = None;
        download(
            &Client::new(),
            &url,
            &path,
            Arc::new(AtomicBool::new(false)),
            |n, _| {
                initial.get_or_insert(n);
            },
        )
        .await
        .unwrap();
        assert_eq!(initial, Some(retained));
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abcdefghijklmnop");
        remove_checkpoint(&path).await;
        tokio::fs::remove_file(path).await.unwrap();
        server.abort();
    }
    #[test]
    fn range_validation() {
        assert_eq!(content_range("bytes 8-15/16"), Some((8, 15, 16)));
        for s in ["bytes 8-16/16", "bytes 9-8/16", "bytes */16", "bytes 0-1/*"] {
            assert!(content_range(s).is_none());
        }
    }
}
