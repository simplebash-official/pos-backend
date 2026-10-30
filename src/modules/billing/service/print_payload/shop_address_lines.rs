use serde_json::Value;

pub(super) fn shop_address_lines(shop: &Value) -> Vec<&str> {
    shop.get("addressLines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}
