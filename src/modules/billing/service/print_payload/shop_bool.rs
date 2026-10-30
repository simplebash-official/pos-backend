use serde_json::Value;

pub(super) fn shop_bool(shop: &Value, field: &str) -> bool {
    shop.get(field).and_then(Value::as_bool).unwrap_or(false)
}
