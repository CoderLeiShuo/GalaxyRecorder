use crate::protocol::{parse_listing_page, Video};
use serde::Serialize;
use std::{collections::HashSet, future::Future};

// Verified directly against A7Listing.read in the original APK.
const PAGE_SIZES: [usize; 6] = [50000, 128, 100, 32, 31, 8];
const MAX_OFFSET: usize = 200000;
const MAX_PAGES: usize = 2000;

#[derive(Debug, Serialize)]
pub struct Listing {
    pub videos: Vec<Video>,
    pub warning: Option<String>,
    #[serde(skip)]
    pub page_size: Option<usize>,
}

#[derive(Debug)]
pub enum PageError {
    Http(u16),
    Other(String),
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(status) => write!(f, "设备返回 HTTP {status}"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

struct AttemptFailure {
    videos: Vec<Video>,
    message: String,
}

/// Retry entire listings for incompatible page sizes, retaining readable data on failure.
pub async fn read<F, Fut, L>(
    category: &str,
    preferred: Option<usize>,
    mut fetch: F,
    mut log: L,
) -> Result<Listing, String>
where
    F: FnMut(usize, usize) -> Fut,
    Fut: Future<Output = Result<String, PageError>>,
    L: FnMut(String),
{
    let mut candidates = Vec::from(PAGE_SIZES);
    if let Some(preferred) = preferred.filter(|size| PAGE_SIZES.contains(size)) {
        candidates.retain(|size| *size != preferred);
        candidates.insert(0, preferred);
    }
    let mut retained = Vec::new();
    let mut seen = HashSet::new();
    let mut failure = String::new();
    for count in candidates {
        match read_all(category, count, &mut fetch, &mut log).await {
            Ok(listing) => return Ok(listing),
            Err(error) => {
                failure = format!("分页 {count}：{}", error.message);
                log(format!("{failure}；将从起点尝试其他分页"));
                for video in error.videos {
                    if seen.insert(video.id.clone()) {
                        retained.push(video);
                    }
                }
            }
        }
    }
    if retained.is_empty() {
        Err(format!(
            "录像列表读取失败，已尝试全部兼容分页；{failure}。可复制连接诊断排查"
        ))
    } else {
        Ok(Listing {
            videos: retained,
            warning: Some(format!("列表尚未完整读取，已保留读取到的录像；{failure}")),
            page_size: None,
        })
    }
}

async fn read_all<F, Fut, L>(
    category: &str,
    initial_count: usize,
    fetch: &mut F,
    log: &mut L,
) -> Result<Listing, AttemptFailure>
where
    F: FnMut(usize, usize) -> Fut,
    Fut: Future<Output = Result<String, PageError>>,
    L: FnMut(String),
{
    let mut videos = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 0;
    let mut count = initial_count;
    let mut repeated = 0;
    let mut single_retry = false;
    let mut rejected = 0;
    let mut reasons = Vec::new();
    for _ in 0..MAX_PAGES {
        log(format!("请求 from={offset}, count={count}"));
        let xml = match fetch(offset, count).await {
            Ok(xml) => xml,
            Err(PageError::Http(500)) if offset > 0 => {
                if count > 1 {
                    count = (count / 2).max(1);
                    log(format!(
                        "HTTP 500；保留已读录像，同一起点缩小分页为 {count}"
                    ));
                    continue;
                }
                if !single_retry {
                    single_retry = true;
                    log("HTTP 500；在同一起点重试单条请求".into());
                    continue;
                }
                return Ok(Listing {
                    videos,
                    warning: Some("列表尚未完整读取：设备在单条重试后仍返回 HTTP 500，已保留读取到的录像，末尾未确认".into()),
                    page_size: None,
                });
            }
            Err(error) => {
                return Err(AttemptFailure {
                    videos,
                    message: error.to_string(),
                })
            }
        };
        log(format!("收到 HTTP 200，响应 {} 字节", xml.len()));
        let page = match parse_listing_page(&xml, category) {
            Ok(page) => page,
            Err(error) => {
                return Err(AttemptFailure {
                    videos,
                    message: error,
                })
            }
        };
        log(format!(
            "原始条目={}，录像={}，非视频={}，拒绝={}{}",
            page.entries,
            page.videos.len(),
            page.non_video,
            page.rejected,
            page.warning()
                .map(|warning| format!("；{warning}"))
                .unwrap_or_default()
        ));
        rejected += page.rejected;
        for reason in page.rejection_reasons {
            if reasons.len() < 3 && !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        let before = videos.len();
        for video in page.videos {
            if seen.insert(video.id.clone()) {
                videos.push(video);
            }
        }
        if page.entries < count {
            if videos.is_empty() && rejected > 0 {
                return Err(AttemptFailure {
                    videos,
                    message: format!(
                        "设备返回了录像条目，但有 {rejected} 条无法读取：{}",
                        reasons.join("；")
                    ),
                });
            }
            return Ok(Listing {
                videos,
                warning: (rejected > 0)
                    .then(|| format!("有 {rejected} 条录像无法读取：{}", reasons.join("；"))),
                page_size: Some(initial_count),
            });
        }
        repeated = if videos.len() == before {
            repeated + 1
        } else {
            0
        };
        if repeated >= 3 {
            if videos.is_empty() {
                return Err(AttemptFailure {
                    videos,
                    message: format!(
                        "连续三页没有可展示录像；被拒绝条目={rejected}；{}",
                        reasons.join("；")
                    ),
                });
            }
            return Ok(Listing {
                videos,
                warning: Some(format!(
                    "设备连续三页没有返回新增录像，已保留读取到的数据；被拒绝条目={rejected}"
                )),
                page_size: None,
            });
        }
        // Advance by raw entries, including photos, duplicates and rejected paths.
        offset += page.entries;
        single_retry = false;
        if offset >= MAX_OFFSET {
            break;
        }
    }
    Ok(Listing {
        videos,
        warning: Some("已达到单次读取上限，列表可能不完整".into()),
        page_size: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Mutex};

    fn xml(names: &[&str]) -> String {
        format!(
            "<list>{}</list>",
            names
                .iter()
                .map(|name| format!("<file><name>{name}</name><size>10</size></file>"))
                .collect::<String>()
        )
    }

    #[tokio::test]
    async fn retries_original_page_sizes_and_restarts_after_truncated_xml() {
        let calls = Mutex::new(Vec::new());
        let result = read(
            "normal",
            None,
            |from, count| {
                calls.lock().unwrap().push((from, count));
                async move {
                    match (from, count) {
                        (_, 50000) => Ok("<list><file>".into()),
                        (_, 128) => Err(PageError::Http(500)),
                        _ => Ok(xml(&["a.mp4"])),
                    }
                }
            },
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(*calls.lock().unwrap(), [(0, 50000), (0, 128), (0, 100)]);
        assert_eq!(result.videos.len(), 1);
        assert_eq!(result.page_size, Some(100));
    }

    #[tokio::test]
    async fn later_http_500_halves_at_same_offset_and_confirms_boundary() {
        let calls = Mutex::new(Vec::new());
        let result = read(
            "normal",
            Some(8),
            |from, count| {
                calls.lock().unwrap().push((from, count));
                async move {
                    if from == 0 {
                        Ok(xml(&["a.mp4"; 8]))
                    } else {
                        Err(PageError::Http(500))
                    }
                }
            },
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            [(0, 8), (8, 8), (8, 4), (8, 2), (8, 1), (8, 1)]
        );
        assert_eq!(result.videos.len(), 1);
        assert!(result.warning.unwrap().contains("末尾未确认"));
        assert_eq!(result.page_size, None);
    }

    #[tokio::test]
    async fn filtered_paths_cannot_be_reported_as_successful_empty_listing() {
        let logs = Mutex::new(Vec::new());
        let result = read(
            "normal",
            None,
            |_, _| async { Ok(xml(&["/mnt/mmc/Event/wrong-category.mp4"])) },
            |line| logs.lock().unwrap().push(line),
        )
        .await;
        assert!(result.unwrap_err().contains("设备返回了录像条目"));
        assert!(logs
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("拒绝=1")));
    }

    #[tokio::test]
    async fn raw_count_advances_past_photos_and_duplicates() {
        let mut replies = VecDeque::from([
            xml(&[
                "a.mp4", "a.mp4", "p.jpg", "p.jpg", "p.jpg", "p.jpg", "p.jpg", "p.jpg",
            ]),
            xml(&["b.mp4"]),
        ]);
        let calls = Mutex::new(Vec::new());
        let result = read(
            "normal",
            Some(8),
            |from, count| {
                calls.lock().unwrap().push((from, count));
                let reply = replies.pop_front().unwrap();
                async { Ok(reply) }
            },
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(*calls.lock().unwrap(), [(0, 8), (8, 8)]);
        assert_eq!(result.videos.len(), 2);
        assert!(result.warning.is_none());
    }

    #[tokio::test]
    async fn failed_strategies_preserve_already_read_videos() {
        let result = read(
            "normal",
            Some(8),
            |from, count| async move {
                if from == 0 && count == 8 {
                    Ok(xml(&["a.mp4"; 8]))
                } else {
                    Err(PageError::Other("模拟断线".into()))
                }
            },
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.videos.len(), 1);
        assert!(result.warning.unwrap().contains("模拟断线"));
    }

    #[tokio::test]
    async fn valid_empty_list_ends_without_retrying() {
        let mut calls = 0;
        let result = read(
            "event",
            None,
            |_, _| {
                calls += 1;
                async { Ok("<list/>".into()) }
            },
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(calls, 1);
        assert!(result.videos.is_empty());
        assert!(result.warning.is_none());
    }

    #[tokio::test]
    async fn truncated_later_page_restarts_from_zero_with_another_strategy() {
        let calls = Mutex::new(Vec::new());
        let result = read(
            "normal",
            Some(8),
            |from, count| {
                calls.lock().unwrap().push((from, count));
                async move {
                    match (from, count) {
                        (0, 8) => Ok(xml(&["a.mp4"; 8])),
                        (8, 8) => Ok("<list><file>".into()),
                        _ => Ok(xml(&["a.mp4", "b.mp4"])),
                    }
                }
            },
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(*calls.lock().unwrap(), [(0, 8), (8, 8), (0, 50000)]);
        assert_eq!(result.videos.len(), 2);
        assert!(result.warning.is_none());
    }

    #[tokio::test]
    async fn mixed_valid_and_rejected_videos_remain_visible_with_warning() {
        let result = read(
            "normal",
            None,
            |_, _| async { Ok(xml(&["a.mp4", "/mnt/mmc/Event/b.mp4"])) },
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.videos.len(), 1);
        assert!(result.warning.unwrap().contains("1 条录像无法读取"));
    }

    #[test]
    fn download_path_is_encoded_without_changing_origin_or_category() {
        let path = crate::protocol::a7_media_path("A & B.mp4", "event").unwrap();
        let mut url = reqwest::Url::parse("http://192.168.43.8:8080").unwrap();
        url.set_path(&path);
        assert_eq!(
            url.as_str(),
            "http://192.168.43.8:8080/mnt/mmc/Event/A%20&%20B.mp4"
        );
    }
}
