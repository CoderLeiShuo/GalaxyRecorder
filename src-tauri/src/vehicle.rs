//! Vehicle identity is independent of the recorder protocol. A7 serves many cars.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VehicleInfo {
    pub name: String,
    pub model: String,
    pub year: Option<u16>,
    pub configuration: Option<String>,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMemory {
    pub key: String,
    pub address: String,
    pub profile: String,
    pub vehicle: Option<VehicleInfo>,
}
fn visible(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= 80 && !value.chars().any(char::is_control))
        .then(|| value.to_string())
}
fn field(fields: &BTreeMap<String, String>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        fields
            .iter()
            .find(|(key, _)| {
                key.to_ascii_lowercase()
                    .replace(['_', '-', ' '], "")
                    .rsplit('.')
                    .next()
                    == Some(name)
            })
            .and_then(|(_, v)| visible(v))
    })
}
pub fn info(fields: &BTreeMap<String, String>) -> Option<VehicleInfo> {
    let explicit = field(
        fields,
        &["vehiclename", "vehiclemodel", "carmodel", "carname", "车型"],
    );
    // A plain `model` in Vyou normally names the recorder board, not the car.
    let model = explicit.or_else(|| {
        field(fields, &["model"]).filter(|m| {
            [
                "星越", "星瑞", "星舰", "银河", "博越", "星耀", "领克", "星愿",
            ]
            .iter()
            .any(|name| m.contains(name))
        })
    })?;
    let model = model.replace("星越 L", "星越L").replace("星越l", "星越L");
    let year = field(
        fields,
        &["modelyear", "vehicleyear", "caryear", "year", "年款"],
    )
    .and_then(|s| parse_year(&s))
    .or_else(|| parse_year(&model));
    let base = strip_year(&model, year);
    let configuration = field(
        fields,
        &[
            "vehicleconfiguration",
            "vehicleconfig",
            "carconfig",
            "trim",
            "carconfiguration",
            "trimlevel",
            "configuration",
            "配置",
        ],
    );
    Some(VehicleInfo {
        name: display_name(&base, year),
        model: base,
        year,
        configuration,
        source: "device".into(),
    })
}
fn parse_year(value: &str) -> Option<u16> {
    let bytes = value.as_bytes();
    for slice in bytes.windows(4) {
        if slice.iter().all(u8::is_ascii_digit) {
            let y = std::str::from_utf8(slice).ok()?.parse::<u16>().ok()?;
            if (2000..=2099).contains(&y) {
                return Some(y);
            }
        }
    }
    let s = value.trim();
    let short = s.get(..2)?.parse::<u16>().ok()?;
    ((20..=99).contains(&short)
        && (s.len() == 2
            || s[2..].starts_with('款')
            || s[2..].starts_with('星')
            || s[2..].starts_with('博')))
    .then_some(2000 + short)
}
fn strip_year(model: &str, year: Option<u16>) -> String {
    let mut value = model.to_string();
    if let Some(year) = year {
        value = value.replace(&year.to_string(), "");
        if value.starts_with(&format!("{:02}", year % 100)) {
            value = value[2..].to_string();
        }
    }
    value
        .replace("年款", "")
        .replace('款', "")
        .trim_matches(|c: char| c.is_whitespace() || c == '·' || c == '-')
        .to_string()
}
pub fn display_name(model: &str, year: Option<u16>) -> String {
    format!(
        "{}{}",
        model,
        year.map(|y| format!("{y}款")).unwrap_or_default()
    )
}
pub fn confirmed(
    model: &str,
    year: Option<u16>,
    configuration: Option<String>,
) -> Result<VehicleInfo, String> {
    let model = visible(model).ok_or("请输入有效车型名称（最多 80 字）")?;
    if year.is_some_and(|y| !(2000..=2099).contains(&y)) {
        return Err("年款请输入 2000–2099".into());
    }
    let configuration = configuration.as_deref().and_then(visible);
    Ok(VehicleInfo {
        name: display_name(&model, year),
        model,
        year,
        configuration,
        source: "confirmed".into(),
    })
}
pub fn device_key(
    address: &str,
    profile: &str,
    fields: &BTreeMap<String, String>,
    network: Option<&str>,
) -> String {
    let identity = field(
        fields,
        &["uuid", "macaddr", "serialnumber", "deviceserial", "sn"],
    );
    let input = if let Some(identity) = identity {
        format!("{profile}|{identity}")
    } else {
        format!("{profile}|{address}|{}", network.unwrap_or(""))
    };
    let hash = input.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("device-{hash:016x}")
}
pub fn merge(old: Option<&VehicleInfo>, new: Option<VehicleInfo>) -> Option<VehicleInfo> {
    match (old, new) {
        (Some(old), Some(mut new)) if old.model == new.model => {
            if new.year.is_none() {
                new.year = old.year;
            }
            if new.configuration.is_none() {
                new.configuration = old.configuration.clone();
            }
            new.name = display_name(&new.model, new.year);
            Some(if old.source == "confirmed" {
                old.clone()
            } else {
                new
            })
        }
        (Some(old), None) => Some(old.clone()),
        (_, new) => new,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    #[test]
    fn protocol_and_hardware_model_do_not_identify_a_car() {
        assert!(info(&fields(&[("model", "A7"), ("version", "2026.1")])).is_none());
        assert!(info(&fields(&[("model", "Vyou")])).is_none());
    }
    #[test]
    fn explicit_car_model_and_year_form_the_home_title() {
        let v = info(&fields(&[
            ("VehicleModel", "星越 L"),
            ("ModelYear", "2026"),
            ("TrimLevel", "旗舰型"),
        ]))
        .unwrap();
        assert_eq!(v.name, "星越L2026款");
        assert_eq!(v.configuration.as_deref(), Some("旗舰型"));
    }
    #[test]
    fn years_are_only_used_when_supplied() {
        assert_eq!(
            info(&fields(&[("CarModel", "星越L")])).unwrap().name,
            "星越L"
        );
        assert_eq!(
            info(&fields(&[("CarModel", "26星越L")])).unwrap().name,
            "星越L2026款"
        );
    }
    #[test]
    fn remembered_confirmation_survives_missing_optional_fields() {
        let old = confirmed("星越L", Some(2026), None).unwrap();
        assert_eq!(
            merge(Some(&old), info(&fields(&[("CarModel", "星越L")]))),
            Some(old)
        );
    }
    #[test]
    fn different_devices_at_the_same_address_have_separate_keys() {
        assert_ne!(
            device_key("192.168.1.1", "A7", &fields(&[("uuid", "one")]), None),
            device_key("192.168.1.1", "A7", &fields(&[("uuid", "two")]), None)
        );
    }
    #[test]
    fn invalid_names_and_years_cannot_be_persisted() {
        assert!(confirmed("", None, None).is_err());
        assert!(confirmed("车\n型", None, None).is_err());
        assert!(confirmed("星越L", Some(9999), None).is_err());
    }
}
