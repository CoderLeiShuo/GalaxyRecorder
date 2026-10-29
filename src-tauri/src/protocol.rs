use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

/// The original A7 probe accepts HTTP 200 + OK without requiring property names.
pub fn a7_probe_accepted(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    body.contains("OK") && !lower.contains("<html") && !lower.contains("<!doctype html")
}

pub fn heartbeat_address(body: &str) -> Option<Ipv4Addr> {
    let normalized = body
        .replace("\\r\\n", "\n")
        .replace("\\n", "\n")
        .replace('\r', "\n");
    normalized.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        if !key.trim().eq_ignore_ascii_case("ip") {
            return None;
        }
        value
            .trim()
            .parse::<Ipv4Addr>()
            .ok()
            .filter(Ipv4Addr::is_private)
    })
}

pub fn same_subnet(address: Ipv4Addr, local: Ipv4Addr, prefix: u8) -> bool {
    if prefix == 0 || prefix > 32 {
        return false;
    }
    let mask = u32::MAX << (32 - prefix);
    u32::from(address) & mask == u32::from(local) & mask
}

/// Cap unusually large LANs, while supporting both halves of ordinary /23 Wi-Fi.
pub fn subnet_hosts(local: Ipv4Addr, prefix: u8) -> Vec<Ipv4Addr> {
    let prefix = prefix.clamp(22, 30);
    let mask = u32::MAX << (32 - prefix);
    let start = u32::from(local) & mask;
    let end = start | !mask;
    (start + 1..end)
        .filter(|ip| *ip != u32::from(local))
        .map(Ipv4Addr::from)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: String,
    pub date: String,
    pub time: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_path: Option<String>,
}

// Wire tests register only listeners owned by the test process. This code does
// not exist in application builds; the production LAN validation stays strict.
#[cfg(test)]
static FIXTURE_ADDRESSES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();
#[cfg(test)]
pub(crate) fn register_fixture(address: &str) {
    assert!(address.starts_with("127.0.0.1:"));
    FIXTURE_ADDRESSES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(address.into());
}
fn local_fixture(address: &str) -> bool {
    #[cfg(test)]
    {
        return FIXTURE_ADDRESSES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .contains(address);
    }
    #[cfg(not(test))]
    {
        let _ = address;
        false
    }
}
pub fn authority(input: &str) -> Result<String, String> {
    let clean = input
        .trim()
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let (ip, port) = clean.split_once(':').map_or((clean, "80"), |p| p);
    let parsed: Ipv4Addr = ip.parse().map_err(|_| "请输入有效的局域网 IPv4 地址")?;
    if !parsed.is_private() && !crate::adapters::factory_ip(parsed) && !local_fixture(clean) {
        return Err("请使用车机热点内的局域网地址".into());
    }
    let port: u16 = port.parse().map_err(|_| "端口必须在 1–65535 之间")?;
    if port == 0 {
        return Err("端口必须在 1–65535 之间".into());
    }
    Ok(if port == 80 {
        parsed.to_string()
    } else {
        format!("{parsed}:{port}")
    })
}

pub fn media_path(input: &str) -> Result<String, String> {
    let path = input.trim().replace('\\', "/");
    if path.is_empty()
        || path.len() > 1024
        || path.contains([':', '?', '#', '%'])
        || path.chars().any(char::is_control)
    {
        return Err("记录仪返回了不支持的录像路径".into());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("录像路径不正确".into());
    }
    let path = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    let lower = path.to_lowercase();
    if ![".mp4", ".mov", ".avi", ".flv"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return Err("不是可识别的录像文件".into());
    }
    Ok(format!("/{}", path.trim_start_matches('/')))
}

/// A7's query properties (DCIM/EVENT) differ from its HTTP directories.
pub fn a7_media_path(input: &str, category: &str) -> Result<String, String> {
    let directory = match category {
        "normal" => "Normal",
        "event" => "Event",
        _ => return Err("不支持的录像分类".into()),
    };
    let path = media_path(input)?;
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    let name = match parts.as_slice() {
        [name] => *name,
        [mnt, mmc, dir, name]
            if mnt.eq_ignore_ascii_case("mnt")
                && mmc.eq_ignore_ascii_case("mmc")
                && dir.eq_ignore_ascii_case(directory) =>
        {
            *name
        }
        _ => return Err("录像路径与 A7 分类目录不一致".into()),
    };
    Ok(format!("/mnt/mmc/{directory}/{name}"))
}

/// Each file is validated according to the adapter that returned its listing.
pub fn video_path(video: &Video) -> Result<String, String> {
    adapter_path(
        &video.path,
        &video.category,
        video.adapter.as_deref().unwrap_or("A7"),
    )
}
pub fn adapter_path(input: &str, category: &str, adapter: &str) -> Result<String, String> {
    match crate::adapters::family(adapter) {
        "A7" => a7_media_path(input, category),
        "E5" => {
            let path = media_path(input)?;
            let path = path
                .strip_prefix("/SD/")
                .or_else(|| path.strip_prefix("/sd/"))
                .unwrap_or(path.trim_start_matches('/'));
            let parts: Vec<_> = path.split('/').collect();
            let (directory, name) = match parts.as_slice() {
                [name] => (if category == "normal" { "MOVIE" } else { "SOS" }, *name),
                [directory, name] => (*directory, *name),
                _ => return Err("E5 录像路径不正确".into()),
            };
            let allowed = match category {
                "normal" => ["MOVIE", "NORMAL"].contains(&directory.to_ascii_uppercase().as_str()),
                "event" => {
                    ["SOS", "LOCK", "EVENT"].contains(&directory.to_ascii_uppercase().as_str())
                }
                _ => false,
            };
            if !allowed {
                return Err("E5 录像路径与分类不一致".into());
            }
            Ok(format!("/SD/{directory}/{name}"))
        }
        "L6" => {
            let path = media_path(input)?;
            if path.trim_start_matches('/').contains('/') {
                return Err("L6 只允许平面录像文件名".into());
            }
            Ok(path)
        }
        _ => Err("不支持的录像方案".into()),
    }
}

#[derive(Debug)]
pub struct ListingPage {
    pub videos: Vec<Video>,
    pub entries: usize,
    pub non_video: usize,
    pub rejected: usize,
    // Reasons deliberately omit device filenames and other response content.
    pub rejection_reasons: Vec<String>,
}

impl ListingPage {
    fn reject(&mut self, reason: impl Into<String>) {
        self.rejected += 1;
        let reason = reason.into();
        if self.rejection_reasons.len() < 3 && !self.rejection_reasons.contains(&reason) {
            self.rejection_reasons.push(reason);
        }
    }

    pub fn warning(&self) -> Option<String> {
        (self.rejected > 0).then(|| {
            format!(
                "有 {} 条录像无法读取：{}",
                self.rejected,
                self.rejection_reasons.join("；")
            )
        })
    }
}

fn stamp(value: &str, name: &str) -> (String, String) {
    let value = value.trim().replace('/', "-");
    if value.len() >= 19
        && value.is_char_boundary(10)
        && value.is_char_boundary(11)
        && value.is_char_boundary(19)
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
    {
        return (value[..10].to_string(), value[11..19].to_string());
    }
    let digits: String = name.chars().filter(char::is_ascii_digit).collect();
    if digits.len() >= 14 && digits.starts_with("20") {
        return (
            format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8]),
            format!("{}:{}:{}", &digits[8..10], &digits[10..12], &digits[12..14]),
        );
    }
    (String::new(), String::new())
}

pub fn parse_listing_page(xml: &str, category: &str) -> Result<ListingPage, String> {
    parse_listing_for(xml, category, "A7")
}
pub fn parse_listing_for(xml: &str, category: &str, adapter: &str) -> Result<ListingPage, String> {
    if !matches!(category, "normal" | "event") {
        return Err("不支持的录像分类".into());
    }
    if xml.len() > 16 * 1024 * 1024 {
        return Err("录像列表响应过大".into());
    }
    let xml = xml.trim().trim_start_matches('\u{feff}').trim();
    let mut reader = Reader::from_str(xml);
    // Text can be split around entity references; trimming each fragment loses spaces.
    reader.config_mut().trim_text(false);
    let mut page = ListingPage {
        videos: Vec::new(),
        entries: 0,
        non_video: 0,
        rejected: 0,
        rejection_reasons: Vec::new(),
    };
    let mut field = String::new();
    let (mut name, mut size, mut time) = (String::new(), String::new(), String::new());
    let mut in_file = false;
    let mut saw_root = false;
    let mut depth: i64 = 0;
    let mut root_closed = false;
    let mut root_tag = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::DocType(_)) => return Err("不支持含 DTD 的录像列表".into()),
            Ok(Event::Start(event)) => {
                if root_closed {
                    return Err("录像列表包含多个根节点".into());
                }
                depth += 1;
                saw_root = true;
                let tag = String::from_utf8_lossy(event.local_name().as_ref()).to_lowercase();
                if depth == 1 {
                    root_tag = tag.clone();
                }
                if tag == "html" || tag == "error" {
                    return Err("设备返回内容不是录像列表".into());
                }
                if tag == "file" {
                    if in_file {
                        return Err("录像列表存在嵌套条目".into());
                    }
                    in_file = true;
                    page.entries += 1;
                    if page.entries > 50000 {
                        return Err("单页录像列表超过上限".into());
                    }
                    name.clear();
                    size.clear();
                    time.clear();
                }
                field = tag;
            }
            Ok(Event::Empty(event)) => {
                if root_closed {
                    return Err("录像列表包含多个根节点".into());
                }
                saw_root = true;
                let tag = String::from_utf8_lossy(event.local_name().as_ref()).to_lowercase();
                if tag == "html" || tag == "error" {
                    return Err("设备返回内容不是录像列表".into());
                }
                if depth == 0 {
                    root_tag = tag.clone();
                    root_closed = true;
                }
                if tag == "file" {
                    page.entries += 1;
                    if page.entries > 50000 {
                        return Err("单页录像列表超过上限".into());
                    }
                    page.reject("录像条目缺少文件名");
                }
            }
            Ok(Event::Text(event)) if in_file => {
                let value = event.decode().map_err(|_| "录像列表编码不正确")?;
                match field.as_str() {
                    "name" => name.push_str(&value),
                    "size" => size.push_str(&value),
                    "time" => time.push_str(&value),
                    _ => {}
                }
            }
            Ok(Event::GeneralRef(event)) => {
                let reference = event.decode().map_err(|_| "录像列表编码不正确")?;
                let escaped = format!("&{reference};");
                let value = quick_xml::escape::unescape(&escaped)
                    .map_err(|_| "录像列表包含无法识别的字符引用")?;
                if in_file {
                    match field.as_str() {
                        "name" => name.push_str(&value),
                        "size" => size.push_str(&value),
                        "time" => time.push_str(&value),
                        _ => {}
                    }
                } else if depth == 0 {
                    return Err("录像列表包含 XML 文档之外的内容".into());
                }
            }
            Ok(Event::CData(event)) if in_file => {
                let value = event.decode().map_err(|_| "录像列表编码不正确")?;
                match field.as_str() {
                    "name" => name.push_str(&value),
                    "size" => size.push_str(&value),
                    "time" => time.push_str(&value),
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                depth -= 1;
                if depth == 0 {
                    root_closed = true;
                }
                if event.local_name().as_ref().eq_ignore_ascii_case(b"file") {
                    in_file = false;
                    let lower = name.trim().to_ascii_lowercase();
                    if name.trim().is_empty() {
                        page.reject("录像条目缺少文件名");
                    } else if ![".mp4", ".mov", ".avi", ".flv"]
                        .iter()
                        .any(|ext| lower.ends_with(ext))
                    {
                        page.non_video += 1;
                    } else if let Ok(path) = adapter_path(&name, category, adapter) {
                        let filename = path.rsplit('/').next().unwrap_or("").to_string();
                        let (date, time) = stamp(&time, &filename);
                        page.videos.push(Video {
                            storage: None,
                            adapter: if adapter == "A7" {
                                None
                            } else {
                                Some(adapter.into())
                            },
                            id: format!("{category}:{path}"),
                            name: filename,
                            path,
                            category: category.into(),
                            date,
                            time,
                            size: size.trim().parse().unwrap_or(0),
                            duration: None,
                            thumbnail: None,
                            exported_path: None,
                            export_error: None,
                            saved_path: None,
                        });
                    } else {
                        page.reject("录像路径无效或与分类目录不一致");
                    }
                }
                field.clear();
            }
            Ok(Event::Eof) => break,
            Ok(Event::Text(event)) if depth == 0 => {
                if !event
                    .decode()
                    .map_err(|_| "录像列表编码不正确")?
                    .trim()
                    .is_empty()
                {
                    return Err("录像列表包含 XML 文档之外的内容".into());
                }
            }
            Err(error) => return Err(format!("录像列表解析失败：{error}")),
            _ => {}
        }
    }
    if !saw_root || depth != 0 || in_file || !root_closed {
        return Err("录像列表未完整返回".into());
    }
    if page.entries == 0
        && !matches!(
            root_tag.as_str(),
            "list"
                | "listing"
                | "filelist"
                | "file_list"
                | "files"
                | "directory"
                | "dir"
                | "dcim"
                | "event"
        )
    {
        return Err("设备返回了无法识别的列表结构，请复制连接诊断".into());
    }
    Ok(page)
}

#[cfg(test)]
fn parse_listing(xml: &str, category: &str) -> Result<Vec<Video>, String> {
    parse_listing_page(xml, category).map(|page| page.videos)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_short_a7_replies_without_property_names() {
        for body in [
            "0\r\nOK\r\n",
            "OK",
            "DVR=ready\nOK",
            "Camera.Menu.FWversion=1.2\nOK",
        ] {
            assert!(a7_probe_accepted(body), "{body}");
        }
        for body in [
            "",
            "ERROR",
            "<html>OK</html>",
            "<!DOCTYPE HTML><body>OK</body>",
        ] {
            assert!(!a7_probe_accepted(body), "{body}");
        }
    }
    #[test]
    fn udp_heartbeat_is_case_insensitive_and_rejects_foreign_addresses() {
        assert_eq!(
            heartbeat_address("iP = 192.168.43.8\r\nrfilename=sample.mp4"),
            Some(Ipv4Addr::new(192, 168, 43, 8))
        );
        assert_eq!(
            heartbeat_address("IP=10.0.0.9\\r\\nrfilename=test"),
            Some(Ipv4Addr::new(10, 0, 0, 9))
        );
        assert!(heartbeat_address("IP=8.8.8.8").is_none());
        assert!(heartbeat_address("IP=192.168.43.8:80").is_none());
    }
    #[test]
    fn discovery_uses_the_actual_wifi_mask() {
        let ip = Ipv4Addr::new(10, 205, 20, 149);
        let hosts = subnet_hosts(ip, 23);
        assert!(hosts.contains(&Ipv4Addr::new(10, 205, 21, 23)));
        assert!(!hosts.contains(&ip));
        assert!(!hosts.contains(&Ipv4Addr::new(10, 205, 22, 1)));
        assert!(same_subnet(Ipv4Addr::new(10, 205, 21, 23), ip, 23));
        assert!(!same_subnet(Ipv4Addr::new(192, 168, 43, 8), ip, 23));
    }
    #[test]
    fn private_address_only() {
        assert_eq!(
            authority("http://192.168.43.123:8080/").unwrap(),
            "192.168.43.123:8080"
        );
        for invalid in [
            "https://192.168.1.1",
            "8.8.8.8",
            "127.0.0.1",
            "192.168.1.1:0",
            "192.168.1.1/path",
            "192.168.1.1:70000",
        ] {
            assert!(authority(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn media_paths_cannot_escape_or_change_origin() {
        assert_eq!(
            media_path("MOVIE/20261027_082640.mp4").unwrap(),
            "/MOVIE/20261027_082640.mp4"
        );
        for path in [
            "../secret.mp4",
            "http://192.168.1.2/a.mp4",
            "/MOVIE/%2e%2e/a.mp4",
            "/MOVIE/a.mp4?x=1",
            "/MOVIE/photo.jpg",
        ] {
            assert!(media_path(path).is_err(), "{path}");
        }
    }
    #[test]
    fn parse_realistic_listing_and_cdata() {
        let xml = "<?xml version=\"1.0\"?><list><file><name><![CDATA[/mnt/mmc/Normal/20261027_082640.mp4]]></name><size>1048576</size><time>2026/10/27 08:26:40</time></file><file><name>/photo.jpg</name></file></list>";
        let items = parse_listing(xml, "normal").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].date, "2026-10-27");
        assert_eq!(items[0].size, 1048576);
        assert_eq!(items[0].duration, None);
    }
    #[test]
    fn rejects_incomplete_and_error_documents() {
        for xml in [
            "",
            "<list><file><name>a.mp4</name>",
            "<html>OK</html>",
            "<!DOCTYPE x><list/>",
            "<list/><other/>",
            "<list><file><name>a.mp4</list>",
            "<response><status>unsupported</status></response>",
            "OK<list/>",
            "<list/>OK",
            "<list><file><name>a&unknown;.mp4</name></file></list>",
        ] {
            assert!(parse_listing(xml, "normal").is_err(), "{xml}");
        }
        assert!(parse_listing("<list/>", "normal").unwrap().is_empty());
    }
    #[test]
    fn filename_timestamp_fallback() {
        let items = parse_listing(
            "<list><file><name>/mnt/mmc/Event/20261027_082106.mp4</name><size>unknown</size></file></list>",
            "event",
        )
        .unwrap();
        assert_eq!(items[0].time, "08:21:06");
        assert_eq!(items[0].size, 0);
    }
    #[test]
    fn pagination_counts_filtered_out_photos() {
        let page = parse_listing_page(
            "<list><file><name>/photo.jpg</name></file><file><name>a.mp4</name></file></list>",
            "normal",
        )
        .unwrap();
        assert_eq!(page.videos.len(), 1);
        assert_eq!(page.entries, 2);
        assert_eq!(page.non_video, 1);
    }

    #[test]
    fn a7_normalizes_dot_segments_and_bare_names_for_both_categories() {
        let page = parse_listing_page("<list><file><name>/mnt/mmc/./Normal/20261027_082640.mp4</name></file><file><name>20261027_082640.mp4</name></file></list>", "normal").unwrap();
        assert_eq!(page.videos.len(), 2);
        assert_eq!(page.videos[0].path, "/mnt/mmc/Normal/20261027_082640.mp4");
        assert_eq!(page.videos[0].id, page.videos[1].id);
        assert_eq!(
            a7_media_path("./clip.mp4", "event").unwrap(),
            "/mnt/mmc/Event/clip.mp4"
        );
        assert_eq!(
            a7_media_path("/clip.mp4", "normal").unwrap(),
            "/mnt/mmc/Normal/clip.mp4"
        );
        for path in [
            "/mnt/mmc/Normal/../Event/a.mp4",
            "/mnt/mmc/Event/a.mp4",
            "http://10.0.0.2/a.mp4",
            "/mnt/mmc/Normal/%2e%2e/a.mp4",
        ] {
            assert!(a7_media_path(path, "normal").is_err(), "{path}");
        }
    }

    #[test]
    fn xml_references_preserve_filename_characters_and_spaces_without_double_decoding() {
        let page = parse_listing_page("<list><file><name>A &amp; B&#46;mp4</name></file><file><name>&amp;amp;&#x2e;mp4</name></file><file><name><![CDATA[A &amp; B.mp4]]></name></file></list>", "normal").unwrap();
        assert_eq!(page.videos.len(), 3);
        assert_eq!(page.videos[0].path, "/mnt/mmc/Normal/A & B.mp4");
        assert_eq!(page.videos[1].name, "&amp;.mp4");
        assert_eq!(page.videos[2].name, "A &amp; B.mp4");
    }

    #[test]
    fn records_rejected_videos_separately_from_non_video_entries() {
        let page = parse_listing_page("<list><file><name>/mnt/mmc/Event/private-file.mp4</name></file><file><name>a.jpg</name></file><file/></list>", "normal").unwrap();
        assert_eq!(page.entries, 3);
        assert_eq!(page.rejected, 2);
        assert_eq!(page.non_video, 1);
        assert!(page.warning().unwrap().contains("2 条"));
        assert!(!page.rejection_reasons.join("").contains("private-file"));
    }

    #[test]
    fn accepts_bom_and_original_large_page_size() {
        let files = "<file><name>a.mp4</name></file>".repeat(50000);
        let page = parse_listing_page(&format!("\u{feff} <list>{files}</list>"), "normal").unwrap();
        assert_eq!(page.entries, 50000);
        assert_eq!(page.videos.len(), 50000);
        assert!(parse_listing_page(&format!("<list>{files}<file/></list>"), "normal").is_err());
    }
}
