use super::super::sales::get_i64_flexible;
use super::category_agg_row::CategoryAggRow;
use super::date_match::date_match;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Retail sales rolled up by product category (or subcategory). A retail line
/// whose product no longer resolves is returned with `category_key = None`.
pub(crate) async fn aggregate_sales_by_category(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    by_subcategory: bool,
) -> AppResult<Vec<CategoryAggRow>> {
    match db {
        Db::Mongo(db) => {
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let line_discount =
                doc! { "$ifNull": ["$items.discountCents", "$items.discount_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };
            let product_key = doc! { "$ifNull": ["$items.productKey", "$items.product_key"] };

            let group_id = if by_subcategory {
                doc! {
                    "categoryKey": "$prod.category_key",
                    "categoryName": "$cat.name",
                    "subcategoryKey": "$prod.subcategory_key",
                    "subcategoryName": "$subcat.name",
                }
            } else {
                doc! {
                    "categoryKey": "$prod.category_key",
                    "categoryName": "$cat.name",
                }
            };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! { "$unwind": "$items" },
                doc! { "$match": { "$expr": { "$eq": [source_type.clone(), "retail"] } } },
                doc! { "$addFields": { "pk": product_key.clone() } },
                doc! {
                    "$lookup": {
                        "from": "products",
                        "localField": "pk",
                        "foreignField": "key",
                        "as": "prod",
                    }
                },
                doc! { "$addFields": { "prod": { "$arrayElemAt": ["$prod", 0] } } },
                doc! {
                    "$lookup": {
                        "from": "categories",
                        "localField": "prod.category_key",
                        "foreignField": "key",
                        "as": "cat",
                    }
                },
                doc! { "$addFields": { "cat": { "$arrayElemAt": ["$cat", 0] } } },
                doc! {
                    "$lookup": {
                        "from": "subcategories",
                        "localField": "prod.subcategory_key",
                        "foreignField": "key",
                        "as": "subcat",
                    }
                },
                doc! { "$addFields": { "subcat": { "$arrayElemAt": ["$subcat", 0] } } },
                doc! {
                    "$group": {
                        "_id": group_id,
                        "units": { "$sum": { "$ifNull": ["$items.quantity", 0] } },
                        "revenue": { "$sum": line_total.clone() },
                        "discount": { "$sum": line_discount.clone() },
                        "cogs": {
                            "$sum": {
                                "$multiply": [
                                    { "$ifNull": [unit_cost.clone(), 0] },
                                    { "$ifNull": ["$items.quantity", 0] },
                                ]
                            }
                        },
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut rows = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let id = row.get_document("_id").ok();
                rows.push(CategoryAggRow {
                    category_key: id
                        .and_then(|d| d.get_str("categoryKey").ok())
                        .map(String::from),
                    category_name: id
                        .and_then(|d| d.get_str("categoryName").ok())
                        .unwrap_or("Uncategorised")
                        .to_string(),
                    subcategory_key: id
                        .and_then(|d| d.get_str("subcategoryKey").ok())
                        .map(String::from),
                    subcategory_name: id
                        .and_then(|d| d.get_str("subcategoryName").ok())
                        .map(String::from),
                    units_sold: get_i64_flexible(&row, "units"),
                    revenue_cents: get_i64_flexible(&row, "revenue"),
                    discount_cents: get_i64_flexible(&row, "discount"),
                    cogs_cents: get_i64_flexible(&row, "cogs"),
                });
            }
            Ok(rows)
        }
        Db::Sqlite(pool) => {
            use sqlx::Row;
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();

            let prod_rows = sqlx::query("SELECT key, category_key, subcategory_key FROM products")
                .fetch_all(pool)
                .await?;
            let mut prod_map: std::collections::HashMap<String, (String, Option<String>)> =
                std::collections::HashMap::new();
            for r in prod_rows {
                let k: String = r.get("key");
                let cat_k: String = r.get("category_key");
                let subcat_k: Option<String> = r.get("subcategory_key");
                prod_map.insert(k, (cat_k, subcat_k));
            }

            let cat_rows = sqlx::query("SELECT key, name FROM categories")
                .fetch_all(pool)
                .await?;
            let mut cat_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for r in cat_rows {
                cat_map.insert(r.get("key"), r.get("name"));
            }

            let subcat_rows = sqlx::query("SELECT key, name FROM subcategories")
                .fetch_all(pool)
                .await?;
            let mut subcat_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for r in subcat_rows {
                subcat_map.insert(r.get("key"), r.get("name"));
            }

            let invoice_rows = sqlx::query(
                "SELECT items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            type GroupKey = (Option<String>, String, Option<String>, Option<String>);
            let mut groups: std::collections::HashMap<GroupKey, CategoryAggRow> =
                std::collections::HashMap::new();

            for row in &invoice_rows {
                let doc = map_sqlite_row_to_document(row);
                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        if source_type != "retail" {
                            continue;
                        }
                        let pk = item
                            .get_str("productKey")
                            .or_else(|_| item.get_str("product_key"))
                            .unwrap_or("");
                        let (cat_key, cat_name, subcat_key, subcat_name) =
                            if let Some((ck, sk)) = prod_map.get(pk) {
                                let cname = cat_map
                                    .get(ck)
                                    .cloned()
                                    .unwrap_or_else(|| "Uncategorised".to_string());
                                let sname = sk.as_ref().and_then(|k| subcat_map.get(k).cloned());
                                (Some(ck.clone()), cname, sk.clone(), sname)
                            } else {
                                (None, "Uncategorised".to_string(), None, None)
                            };

                        let gkey = if by_subcategory {
                            (
                                cat_key.clone(),
                                cat_name.clone(),
                                subcat_key.clone(),
                                subcat_name.clone(),
                            )
                        } else {
                            (cat_key.clone(), cat_name.clone(), None, None)
                        };

                        let line_total = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")
                        } else {
                            get_i64_flexible(item, "total_cents")
                        };
                        let line_discount = if item.contains_key("discountCents") {
                            get_i64_flexible(item, "discountCents")
                        } else {
                            get_i64_flexible(item, "discount_cents")
                        };
                        let unit_cost = if item.contains_key("unitCostCents") {
                            get_i64_flexible(item, "unitCostCents")
                        } else {
                            get_i64_flexible(item, "unit_cost_cents")
                        };
                        let qty = get_i64_flexible(item, "quantity");

                        let entry = groups.entry(gkey).or_insert_with(|| CategoryAggRow {
                            category_key: cat_key,
                            category_name: cat_name,
                            subcategory_key: if by_subcategory { subcat_key } else { None },
                            subcategory_name: if by_subcategory { subcat_name } else { None },
                            ..Default::default()
                        });
                        entry.units_sold += qty;
                        entry.revenue_cents += line_total;
                        entry.discount_cents += line_discount;
                        entry.cogs_cents += unit_cost * qty;
                    }
                }
            }
            Ok(groups.into_values().collect())
        }
    }
}
