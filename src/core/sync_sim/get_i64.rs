use serde_json::Value;

pub(super) fn get_i64(p: &Value, key: &str) -> i64 {
    p.get(key).and_then(Value::as_i64).unwrap_or(0)
}
