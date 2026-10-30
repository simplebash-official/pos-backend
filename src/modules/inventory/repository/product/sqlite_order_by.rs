use super::min_selling_price::MIN_SELLING_PRICE;

use mongodb::bson::Document;

const MIN_PRICE_SQL: &str = "(SELECT MIN(b.selling_price_cents) FROM stock_batches b \
     WHERE b.product_key = p.key AND b.remaining_quantity > 0 AND b.deleted_at IS NULL)";

/// Builds a deterministic `ORDER BY` from a (whitelisted) Mongo-style sort
/// document. Every key in the document is honoured in order; text columns
/// compare case-insensitively and `_id` maps to the `id` column. Field names
/// only ever come from `sort_field_for`'s whitelist, never from client input.
pub(super) fn sqlite_order_by(sort: &Document) -> String {
    let mut parts: Vec<String> = sort
        .iter()
        .map(|(field, order)| {
            if field == MIN_SELLING_PRICE {
                let dir = if order.as_i32() == Some(-1) {
                    "DESC"
                } else {
                    "ASC"
                };
                return format!("{MIN_PRICE_SQL} {dir}");
            }
            let column = if field == "_id" { "id" } else { field.as_str() };
            let dir = if order.as_i32() == Some(-1) {
                "DESC"
            } else {
                "ASC"
            };
            let collate = if matches!(column, "name" | "sku") {
                " COLLATE NOCASE"
            } else {
                ""
            };
            format!("p.{column}{collate} {dir}")
        })
        .collect();
    if parts.is_empty() {
        parts.push("p.name COLLATE NOCASE ASC".to_string());
    }
    format!("ORDER BY {}", parts.join(", "))
}
