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
    pub duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_path: Option<String>,
}

pub fn authority(input: &str) -> Result<String, String> {
    let clean = input
        .trim()
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let (ip, port) = clean.split_once(':').map_or((clean, "80"), |p| p);
    let parsed: Ipv4Addr = ip.parse().map_err(|_| "请输入有效的局域网 IPv4 地址")?;
    if !parsed.is_private() {
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
    if path.split('/').any(|part| part == "." || part == "..") {
        return Err("录像路径不正确".into());
    }
    let lower = path.to_lowercase();
    if ![".mp4", ".mov", ".avi", ".flv"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return Err("不是可识别的录像文件".into());
    }
    Ok(format!("/{}", path.trim_start_matches('/')))
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

pub fn parse_listing_page(xml: &str, category: &str) -> Result<(Vec<Video>, usize), String> {
    if xml.len() > 8 * 1024 * 1024 {
        return Err("录像列表响应过大".into());
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut videos = Vec::new();
    let mut field = String::new();
    let (mut name, mut size, mut time) = (String::new(), String::new(), String::new());
    let mut in_file = false;
    let mut saw_root = false;
    let mut depth: i64 = 0;
    let mut root_closed = false;
    let mut entries = 0;
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
                if tag == "html" || tag == "error" {
                    return Err("设备返回内容不是录像列表".into());
                }
                if tag == "file" {
                    if in_file {
                        return Err("录像列表存在嵌套条目".into());
                    }
                    in_file = true;
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
                    root_closed = true;
                }
            }
            Ok(Event::Text(event)) if in_file => {
                let decoded = event.decode().map_err(|_| "录像列表编码不正确")?;
                let value =
                    quick_xml::escape::unescape(&decoded).map_err(|_| "录像列表转义不正确")?;
                match field.as_str() {
                    "name" => name.push_str(&value),
                    "size" => size.push_str(&value),
                    "time" => time.push_str(&value),
                    _ => {}
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
                    entries += 1;
                    in_file = false;
                    // Device listings can include still images and non-media entries.
                    if let Ok(path) = media_path(&name) {
                        let filename = path.rsplit('/').next().unwrap_or("").to_string();
                        let (date, time) = stamp(&time, &filename);
                        videos.push(Video {
                            id: format!("{category}:{path}"),
                            name: filename,
                            path,
                            category: category.into(),
                            date,
                            time,
                            size: size.trim().parse().unwrap_or(0),
                            duration: None,
                            saved_path: None,
                        });
                    }
                    if videos.len() > 5000 {
                        return Err("单页录像列表超过上限".into());
                    }
                }
                field.clear();
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("录像列表解析失败：{error}")),
            _ => {}
        }
    }
    if !saw_root || depth != 0 || in_file || !root_closed {
        return Err("录像列表未完整返回".into());
    }
    Ok((videos, entries))
}

#[cfg(test)]
fn parse_listing(xml: &str, category: &str) -> Result<Vec<Video>, String> {
    parse_listing_page(xml, category).map(|(videos, _)| videos)
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
        let xml = "<?xml version=\"1.0\"?><list><file><name><![CDATA[/MOVIE/20261027_082640.mp4]]></name><size>1048576</size><time>2026/10/27 08:26:40</time></file><file><name>/photo.jpg</name></file></list>";
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
        ] {
            assert!(parse_listing(xml, "normal").is_err(), "{xml}");
        }
        assert!(parse_listing("<list/>", "normal").unwrap().is_empty());
    }
    #[test]
    fn filename_timestamp_fallback() {
        let items = parse_listing(
            "<list><file><name>/SOS/20261027_082106.mp4</name><size>unknown</size></file></list>",
            "event",
        )
        .unwrap();
        assert_eq!(items[0].time, "08:21:06");
        assert_eq!(items[0].size, 0);
    }
    #[test]
    fn pagination_counts_filtered_out_photos() {
        let (videos, entries) = parse_listing_page("<list><file><name>/photo.jpg</name></file><file><name>/MOVIE/a.mp4</name></file></list>", "normal").unwrap();
        assert_eq!(videos.len(), 1);
        assert_eq!(entries, 2);
    }
}
