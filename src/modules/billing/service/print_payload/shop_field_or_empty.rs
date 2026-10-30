use super::shop_field::shop_field;

use serde_json::Value;

pub(super) fn shop_field_or_empty<'a>(shop: &'a Value, field: &str) -> &'a str {
    shop_field(shop, field).unwrap_or("")
}
