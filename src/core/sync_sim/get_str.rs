use serde_json::Value;

pub(super) fn get_str(p: &Value, key: &str) -> String {
    p.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
