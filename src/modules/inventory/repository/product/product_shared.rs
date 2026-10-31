//! Helpers shared by the product repository files: collection handle, row mapping, filters.

use crate::clients::sqlite::{SqlFilter, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::{AppError, AppResult};
use crate::modules::inventory::model::ProductDocument;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document};
use sqlx::Row;

pub(super) fn build_product_where_clause(filter: &Document) -> SqlFilter {
    let mut conditions = vec!["p.deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    if let Ok(key_in) = filter.get_document("key")
        && let Ok(arr) = key_in.get_array("$in")
    {
        let placeholders: Vec<_> = arr.iter().map(|_| "?").collect();
        conditions.push(format!("p.key IN ({})", placeholders.join(",")));
        for val in arr {
            if let Some(s) = val.as_str() {
                bindings.push(s.to_string());
            }
        }
    }

    if let Ok(cat_key) = filter.get_str("category_key") {
        conditions.push("p.category_key = ?".to_string());
        bindings.push(cat_key.to_string());
    }

    if let Ok(subcat_key) = filter.get_str("subcategory_key") {
        conditions.push("p.subcategory_key = ?".to_string());
        bindings.push(subcat_key.to_string());
    }

    if let Ok(and_arr) = filter.get_array("$and") {
        for item in and_arr {
            if let Some(item_doc) = item.as_document() {
                if let Ok(cat_key) = item_doc.get_str("category_key") {
                    conditions.push("p.category_key = ?".to_string());
                    bindings.push(cat_key.to_string());
                }
                if let Ok(subcat_key) = item_doc.get_str("subcategory_key") {
                    conditions.push("p.subcategory_key = ?".to_string());
                    bindings.push(subcat_key.to_string());
                }
                if item_doc.contains_key("$expr") {
                    conditions.push("p.stock_quantity <= p.min_stock_threshold".to_string());
                }
                if let Ok(or_arr) = item_doc.get_array("$or") {
                    let mut or_parts = Vec::new();
                    for or_item in or_arr {
                        if let Some(or_doc) = or_item.as_document() {
                            if let Ok(pat) = or_doc.get_document("name") {
                                if let Some(like) = like_pattern(pat) {
                                    or_parts.push("p.name LIKE ? ESCAPE '\\'".to_string());
                                    bindings.push(like);
                                }
                            } else if let Ok(pat) = or_doc.get_document("sku") {
                                if let Some(like) = like_pattern(pat) {
                                    or_parts.push("p.sku LIKE ? ESCAPE '\\'".to_string());
                                    bindings.push(like);
                                }
                            } else if let Ok(pat) = or_doc.get_document("barcode") {
                                if let Some(like) = like_pattern(pat) {
                                    or_parts.push("p.barcode LIKE ? ESCAPE '\\'".to_string());
                                    bindings.push(like);
                                }
                            } else if let Ok(cat_in) = or_doc.get_document("category_key") {
                                if let Ok(arr) = cat_in.get_array("$in") {
                                    let ph: Vec<_> = arr.iter().map(|_| "?").collect();
                                    or_parts.push(format!("p.category_key IN ({})", ph.join(",")));
                                    for v in arr {
                                        if let Some(s) = v.as_str() {
                                            bindings.push(s.to_string());
                                        }
                                    }
                                }
                            } else if let Ok(subcat_in) = or_doc.get_document("subcategory_key")
                                && let Ok(arr) = subcat_in.get_array("$in")
                            {
                                let ph: Vec<_> = arr.iter().map(|_| "?").collect();
                                or_parts.push(format!("p.subcategory_key IN ({})", ph.join(",")));
                                for v in arr {
                                    if let Some(s) = v.as_str() {
                                        bindings.push(s.to_string());
                                    }
                                }
                            }
                        }
                    }
                    if !or_parts.is_empty() {
                        conditions.push(format!("({})", or_parts.join(" OR ")));
                    }
                }
            }
        }
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    SqlFilter {
        clause: where_clause,
        bindings,
    }
}

/// The SQLite `LIKE` pattern (`%text%`, with `%`, `_` and `\` escaped for
/// `ESCAPE '\'`) for a `{ "$regex": … }` search clause. `service::product`
/// sends a BSON regex built by `build_bson_regex`, whose text is regex-escaped,
/// so the escaping is undone first; a plain string is taken as it is.
pub(super) fn like_pattern(clause: &Document) -> Option<String> {
    let regex = match clause.get("$regex")? {
        Bson::RegularExpression(regex) => regex.pattern.to_string(),
        Bson::String(text) => text.clone(),
        _ => return None,
    };
    let mut text = String::with_capacity(regex.len() + 2);
    text.push('%');
    let mut chars = regex.chars();
    while let Some(c) = chars.next() {
        // `regex_escape` puts a backslash before each special character.
        let literal = match c {
            '\\' => chars.next().unwrap_or('\\'),
            other => other,
        };
        if matches!(literal, '%' | '_' | '\\') {
            text.push('\\');
        }
        text.push(literal);
    }
    text.push('%');
    Some(text)
}

/// Sort field standing for "a product's cheapest in-stock price". A product
/// stores no price (its batches do), so this is computed per row while
/// sorting: `min_price_stages` (Mongo) / `MIN_PRICE_SQL` (SQLite).
pub(crate) const MIN_SELLING_PRICE: &str = "min_selling_price_cents";

pub(super) fn products(db: &TenantDatabase) -> ScopedCollection<ProductDocument> {
    db.collection("products")
}

pub(super) fn row_to_product_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProductDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let barcode_source_str: Option<String> = row.get("barcode_source");
    let barcode_source = barcode_source_str
        .map(|text| {
            text.parse().map_err(|_| {
                AppError::internal(format!("stored barcode source `{text}` is unknown"))
            })
        })
        .transpose()?;
    let is_serialized_int: i64 = row.get("is_serialized");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(ProductDocument {
        id: object_id,
        key: row.get("key"),
        sku: row.get("sku"),
        barcode: row.get("barcode"),
        barcode_source,
        name: row.get("name"),
        category_key: row.get("category_key"),
        subcategory_key: row.get("subcategory_key"),
        stock_quantity: row.get("stock_quantity"),
        min_stock_threshold: row.get("min_stock_threshold"),
        is_serialized: is_serialized_int != 0,
        warranty_months: row.get("warranty_months"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str)?,
        updated_at: to_bson_datetime(&updated_at_str)?,
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)).transpose()?,
        updated_by_device: row.get("updated_by_device"),
    })
}

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
