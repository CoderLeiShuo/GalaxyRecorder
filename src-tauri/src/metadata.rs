use crate::{
    connection::client,
    media::{remote_url, MediaServer},
    protocol::Video,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    time::Duration,
};
use tauri::{AppHandle, Manager};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    pub duration: Option<f64>,
    pub size: Option<u64>,
    pub thumbnail: Option<String>,
    pub warning: Option<String>,
}

fn box_header(bytes: &[u8], offset: usize, total: u64) -> Result<(u64, [u8; 4], usize), String> {
    let head = bytes.get(offset..offset + 8).ok_or("MP4 索引不完整")?;
    let short = u32::from_be_bytes(head[0..4].try_into().unwrap());
    let kind = head[4..8].try_into().unwrap();
    let (size, header) = match short {
        0 => (total, 8),
        1 => (
            u64::from_be_bytes(
                bytes
                    .get(offset + 8..offset + 16)
                    .ok_or("扩展索引不完整")?
                    .try_into()
                    .unwrap(),
            ),
            16,
        ),
        _ => (short as u64, 8),
    };
    if size < header as u64 {
        return Err("MP4 索引长度不正确".into());
    }
    Ok((size, kind, header))
}
pub fn moov_duration(bytes: &[u8]) -> Result<f64, String> {
    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let (size, kind, header) = box_header(bytes, offset, (bytes.len() - offset) as u64)?;
        let end = offset
            .checked_add(size as usize)
            .filter(|end| *end <= bytes.len())
            .ok_or("MP4 索引越界")?;
        if &kind == b"mvhd" {
            let payload = &bytes[offset + header..end];
            let version = *payload.first().ok_or("缺少 MP4 版本")?;
            let read32 = |i| {
                payload
                    .get(i..i + 4)
                    .map(|v| u32::from_be_bytes(v.try_into().unwrap()))
                    .ok_or("时长字段不完整")
            };
            let (scale, duration) = if version == 0 {
                (read32(12)?, read32(16)? as u64)
            } else if version == 1 {
                (
                    read32(20)?,
                    u64::from_be_bytes(
                        payload
                            .get(24..32)
                            .ok_or("时长字段不完整")?
                            .try_into()
                            .unwrap(),
                    ),
                )
            } else {
                return Err("不支持的 MP4 索引版本".into());
            };
            if scale == 0 || duration == u64::MAX {
                return Err("MP4 未提供有效时长".into());
            }
            return Ok(duration as f64 / scale as f64);
        }
        offset = end;
    }
    Err("没有找到录像时长".into())
}
async fn range(url: &str, start: u64, count: u64) -> Result<(Vec<u8>, Option<u64>), String> {
    let mut response = client()?
        .get(url)
        .header("Range", format!("bytes={start}-{}", start + count - 1))
        .header("Accept-Encoding", "identity")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("索引请求 HTTP {}", response.status()));
    }
    let total = response
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit('/').next())
        .and_then(|v| v.parse().ok())
        .or_else(|| {
            if response.status().as_u16() == 200 {
                response.content_length()
            } else {
                None
            }
        });
    if start > 0 && response.status().as_u16() != 206 {
        return Err("设备不支持按位置读取录像索引".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        let remaining = count as usize - bytes.len();
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if bytes.len() >= count as usize {
            break;
        }
    }
    Ok((bytes, total))
}
async fn duration(url: &str, size: u64) -> Result<f64, String> {
    let mut offset = 0;
    let mut total = size;
    for _ in 0..128 {
        let (bytes, length) = range(url, offset, 64 * 1024).await?;
        if let Some(length) = length {
            total = length;
        }
        let (length, kind, header) = box_header(&bytes, 0, total.saturating_sub(offset))?;
        if &kind == b"moov" {
            if length > 16 * 1024 * 1024 {
                return Err("录像索引超过读取上限".into());
            }
            let bytes = if length as usize <= bytes.len() {
                bytes
            } else {
                range(url, offset, length).await?.0
            };
            return moov_duration(bytes.get(header..length as usize).ok_or("录像索引不完整")?);
        }
        offset = offset.checked_add(length).ok_or("索引长度溢出")?;
        if total > 0 && offset >= total {
            break;
        }
    }
    Err("没有找到 MP4 时长索引".into())
}

#[tauri::command]
pub async fn video_metadata(
    app: AppHandle,
    address: Option<String>,
    mut video: Video,
    server: tauri::State<'_, MediaServer>,
) -> Result<Metadata, String> {
    if video.saved_path.is_some() {
        video = crate::library::find(&app, &video.id).await?;
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (
        "with-size-v1",
        &address,
        &video.id,
        &video.path,
        &video.saved_path,
        video.size,
    )
        .hash(&mut hash);
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("metadata")
        .join(format!("{:016x}.json", hash.finish()));
    if let Ok(bytes) = tokio::fs::read(&cache).await {
        if let Ok(value) = serde_json::from_slice::<Metadata>(&bytes) {
            return Ok(value);
        }
    }
    let mut result = Metadata {
        size: Some(video.size).filter(|s| *s > 0),
        duration: video.duration,
        thumbnail: video.thumbnail.clone(),
        warning: None,
    };
    if video.saved_path.is_some() {
        let source = server.video(None, &video, &app).await?;
        result.size = source.size;
    }
    if let Some(address) = address.as_deref().filter(|_| video.saved_path.is_none()) {
        crate::network::prepare(&app, false).await?;
        if result.size.is_none() {
            result.size = crate::media::remote_size(&remote_url(address, &video, false)?)
                .await
                .ok();
        }
        if result.thumbnail.is_none() {
            let mut response = client()?
                .get(remote_url(address, &video, true)?)
                .timeout(Duration::from_secs(6))
                .send()
                .await
                .map_err(|e| e.to_string())
                .ok();
            if let Some(response) = response.as_mut().filter(|r| r.status().is_success()) {
                let mut image = Vec::new();
                while let Ok(Some(chunk)) = response.chunk().await {
                    if image.len() + chunk.len() > 2 * 1024 * 1024 {
                        image.clear();
                        break;
                    }
                    image.extend_from_slice(&chunk);
                }
                let mime = if image.starts_with(&[0xff, 0xd8]) {
                    Some("image/jpeg")
                } else if image.starts_with(b"\x89PNG") {
                    Some("image/png")
                } else {
                    None
                };
                if let Some(mime) = mime {
                    result.thumbnail = Some(format!(
                        "data:{mime};base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(image)
                    ));
                }
            }
        }
        if result.duration.is_none() {
            match duration(&remote_url(address, &video, false)?, video.size).await {
                Ok(value) => result.duration = Some(value),
                Err(error) => result.warning = Some(error),
            }
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if result.duration.is_none() || result.thumbnail.is_none() {
        if let Ok(source) = server.video(address.as_deref(), &video, &app).await {
            if let Ok(native) =
                crate::mobile::call(&app, "metadata", serde_json::json!({"source":source.url}))
                    .await
            {
                if result.duration.is_none() {
                    result.duration = native["duration"].as_f64();
                }
                if result.thumbnail.is_none() {
                    result.thumbnail = native["thumbnail"].as_str().map(str::to_string);
                }
            }
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if result.duration.is_none() || result.thumbnail.is_none() {
        if let Ok(source) = server.video(address.as_deref(), &video, &app).await {
            if result.thumbnail.is_none() {
                let mut command = tokio::process::Command::new(crate::media::ffmpeg());
                command.args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-ss",
                    "0.5",
                    "-i",
                    &source.url,
                    "-frames:v",
                    "1",
                    "-vf",
                    "scale=480:-2",
                    "-f",
                    "image2pipe",
                    "-vcodec",
                    "mjpeg",
                    "pipe:1",
                ]);
                command.kill_on_drop(true);
                if let Ok(Ok(output)) =
                    tokio::time::timeout(Duration::from_secs(15), command.output()).await
                {
                    if output.status.success() && output.stdout.starts_with(&[0xff, 0xd8]) {
                        result.thumbnail = Some(format!(
                            "data:image/jpeg;base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(output.stdout)
                        ));
                    }
                }
            }
            if result.duration.is_none() {
                let binary = crate::media::ffmpeg().with_file_name(if cfg!(windows) {
                    "ffprobe.exe"
                } else {
                    "ffprobe"
                });
                let mut command = tokio::process::Command::new(binary);
                command.args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-of",
                    "default=noprint_wrappers=1:nokey=1",
                    &source.url,
                ]);
                command.kill_on_drop(true);
                if let Ok(Ok(output)) =
                    tokio::time::timeout(Duration::from_secs(15), command.output()).await
                {
                    result.duration = String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite() && *v > 0.0);
                }
            }
        }
    }
    if result.duration.is_some() && result.thumbnail.is_some() {
        tokio::fs::create_dir_all(cache.parent().unwrap())
            .await
            .map_err(|e| e.to_string())?;
        tokio::fs::write(
            cache,
            serde_json::to_vec(&result).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_movie_header_versions_and_rejects_truncation() {
        for version in [0, 1] {
            let mut payload = vec![0; 40];
            payload[0] = version;
            let i = if version == 0 { 12 } else { 20 };
            payload[i..i + 4].copy_from_slice(&1000u32.to_be_bytes());
            if version == 0 {
                payload[16..20].copy_from_slice(&123456u32.to_be_bytes());
            } else {
                payload[24..32].copy_from_slice(&123456u64.to_be_bytes());
            }
            let mut b = Vec::from(((payload.len() + 8) as u32).to_be_bytes());
            b.extend_from_slice(b"mvhd");
            b.extend(payload);
            assert!((moov_duration(&b).unwrap() - 123.456).abs() < 0.001);
            assert!(moov_duration(&b[..15]).is_err());
        }
    }
    #[tokio::test]
    async fn movie_at_end_is_read_by_range_without_downloading_mdat() {
        use axum::{
            extract::State,
            http::{HeaderMap, StatusCode},
            routing::get,
        };
        use std::sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&16u32.to_be_bytes());
        bytes.extend_from_slice(b"ftypisom0000");
        bytes.extend_from_slice(&1_000_000u32.to_be_bytes());
        bytes.extend_from_slice(b"mdat");
        bytes.resize(1_000_016, 0);
        let mut payload = vec![0; 20];
        payload[12..16].copy_from_slice(&1000u32.to_be_bytes());
        payload[16..20].copy_from_slice(&12000u32.to_be_bytes());
        bytes.extend_from_slice(&36u32.to_be_bytes());
        bytes.extend_from_slice(b"moov");
        bytes.extend_from_slice(&28u32.to_be_bytes());
        bytes.extend_from_slice(b"mvhd");
        bytes.extend(payload);
        let transferred = Arc::new(AtomicU64::new(0));
        let state = (Arc::new(bytes), transferred.clone());
        let router = axum::Router::new()
            .route(
                "/video",
                get(
                    |State((bytes, count)): State<(Arc<Vec<u8>>, Arc<AtomicU64>)>,
                     headers: HeaderMap| async move {
                        let (start, length, _) = crate::media::byte_range(
                            headers.get("range").and_then(|v| v.to_str().ok()),
                            bytes.len() as u64,
                        )
                        .unwrap();
                        count.fetch_add(length, Ordering::Relaxed);
                        (
                            StatusCode::PARTIAL_CONTENT,
                            [(
                                "Content-Range",
                                format!(
                                    "bytes {start}-{}/{total}",
                                    start + length - 1,
                                    total = bytes.len()
                                ),
                            )],
                            bytes[start as usize..(start + length) as usize].to_vec(),
                        )
                    },
                ),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        assert_eq!(
            duration(&format!("http://{address}/video"), 0)
                .await
                .unwrap(),
            12.0
        );
        assert!(transferred.load(Ordering::Relaxed) < 150000);
        task.abort();
    }
}
