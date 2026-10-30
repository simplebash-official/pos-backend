use serde_json::Value;

/// `invoice.shop_profile_snapshot` is the frontend's `ShopProfile` object,
/// embedded verbatim at sale-completion time (D2) — this just reads fields
/// off it by name by way of `serde_json::Value` indexing, since the
/// snapshot's shape is owned by the frontend, not modeled as a Rust struct
/// here (no backend shop-profile module exists, deliberately — see D2).
pub(super) fn shop_field<'a>(shop: &'a Value, field: &str) -> Option<&'a str> {
    shop.get(field).and_then(Value::as_str)
}
