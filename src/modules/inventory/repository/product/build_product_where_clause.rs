use super::like_pattern::like_pattern;
use crate::clients::sqlite::SqlFilter;

use mongodb::bson::Document;

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
