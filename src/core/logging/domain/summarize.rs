use serde::Serialize;
use serde_json::Value;

/// Pull a compact identity out of a service result: its `key` (and `number`
/// / `invoiceNumber` style business numbers when present), or the item count
/// for list results.
pub(super) fn summarize<T: Serialize>(
    value: &T,
) -> (Option<String>, Option<String>, Option<usize>) {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => {
            let key = map.get("key").and_then(Value::as_str).map(str::to_string);
            let number = ["invoiceNumber", "number", "code", "sku"]
                .iter()
                .find_map(|k| map.get(*k))
                .and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });
            (key, number, None)
        }
        Ok(Value::Array(items)) => (None, None, Some(items.len())),
        _ => (None, None, None),
    }
}
