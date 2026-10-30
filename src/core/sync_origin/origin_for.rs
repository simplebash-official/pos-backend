use super::cloud_origin::CLOUD_ORIGIN;
use super::web_prefix::WEB_PREFIX;

/// The origin for a request: a registered device's id, else the browser's
/// `X-Device-Id` (prefixed), else `cloud`.
pub fn origin_for(device_id: Option<&str>, header_device: Option<&str>) -> String {
    if let Some(d) = device_id.map(str::trim).filter(|d| !d.is_empty()) {
        return d.to_string();
    }
    match header_device.map(str::trim).filter(|d| !d.is_empty()) {
        Some(h) => format!("{WEB_PREFIX}{h}"),
        None => CLOUD_ORIGIN.to_string(),
    }
}
