use super::get_i64_flexible::get_i64_flexible;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;
use crate::domain::reports::TopProductEntry;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Aggregates top selling products by units sold or revenue.
pub(crate) async fn aggregate_top_products(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: u64,
    sort_by: &str,
) -> AppResult<Vec<TopProductEntry>> {
    match db {
        Db::Mongo(db) => {
            let sort_field = if sort_by == "quantity" {
                "units_sold"
            } else {
                "total_revenue_cents"
            };

            let pipeline = vec![
                doc! {
                    "$match": {
                        "status": { "$ne": "voided" },
                        "created_at": {
                            "$gte": BsonDateTime::from_chrono(start),
                            "$lt": BsonDateTime::from_chrono(end),
                        }
                    }
                },
                doc! { "$unwind": "$items" },
                doc! {
                    "$group": {
                        "_id": {
                            "product_key": { "$ifNull": ["$items.productKey", "$items.product_key"] },
                            "name": "$items.name",
                            "sku": "$items.sku",
                            "source_type": { "$ifNull": ["$items.sourceType", "$items.source_type"] },
                        },
                        "units_sold": { "$sum": "$items.quantity" },
                        "total_revenue_cents": { "$sum": { "$ifNull": ["$items.totalCents", "$items.total_cents"] } },
                        "total_discount_cents": { "$sum": { "$ifNull": ["$items.discountCents", "$items.discount_cents"] } },
                    }
                },
                doc! { "$sort": { sort_field: -1 } },
                doc! { "$limit": limit as i64 },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut products = Vec::new();

            while let Some(doc) = cursor.try_next().await? {
                if let Ok(id_doc) = doc.get_document("_id") {
                    let product_key = id_doc.get_str("product_key").ok().map(String::from);
                    let name = id_doc.get_str("name").unwrap_or("Unknown Item").to_string();
                    let sku = id_doc.get_str("sku").ok().map(String::from);
                    let source_type = id_doc
                        .get_str("source_type")
                        .unwrap_or("retail")
                        .to_string();
                    let units_sold = get_i64_flexible(&doc, "units_sold");
                    let total_revenue_cents = get_i64_flexible(&doc, "total_revenue_cents");
                    let total_discount_cents = get_i64_flexible(&doc, "total_discount_cents");

                    products.push(TopProductEntry {
                        product_key,
                        name,
                        sku,
                        source_type,
                        units_sold,
                        total_revenue_cents,
                        total_discount_cents,
                    });
                }
            }

            Ok(products)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            #[derive(Debug, Default)]
            struct Agg {
                name: String,
                sku: Option<String>,
                source_type: String,
                units_sold: i64,
                total_revenue_cents: i64,
                total_discount_cents: i64,
            }

            let mut grouped: std::collections::HashMap<(Option<String>, String), Agg> =
                std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                if let Ok(items) = doc.get_array("items") {
                    for item in items {
                        if let Some(idoc) = item.as_document() {
                            let product_key = idoc
                                .get_str("productKey")
                                .or_else(|_| idoc.get_str("product_key"))
                                .ok()
                                .map(String::from);
                            let name = idoc.get_str("name").unwrap_or("Unknown Item").to_string();
                            let sku = idoc.get_str("sku").ok().map(String::from);
                            let source_type = idoc
                                .get_str("sourceType")
                                .or_else(|_| idoc.get_str("source_type"))
                                .unwrap_or("retail")
                                .to_string();
                            let quantity = get_i64_flexible(idoc, "quantity");
                            let total_cents = if idoc.contains_key("totalCents") {
                                get_i64_flexible(idoc, "totalCents")
                            } else {
                                get_i64_flexible(idoc, "total_cents")
                            };
                            let discount_cents = if idoc.contains_key("discountCents") {
                                get_i64_flexible(idoc, "discountCents")
                            } else {
                                get_i64_flexible(idoc, "discount_cents")
                            };

                            let key = (product_key.clone(), name.clone());
                            let entry = grouped.entry(key).or_insert_with(|| Agg {
                                name,
                                sku,
                                source_type,
                                units_sold: 0,
                                total_revenue_cents: 0,
                                total_discount_cents: 0,
                            });
                            entry.units_sold += quantity;
                            entry.total_revenue_cents += total_cents;
                            entry.total_discount_cents += discount_cents;
                        }
                    }
                }
            }

            let mut products: Vec<TopProductEntry> = grouped
                .into_iter()
                .map(|((product_key, _), agg)| TopProductEntry {
                    product_key,
                    name: agg.name,
                    sku: agg.sku,
                    source_type: agg.source_type,
                    units_sold: agg.units_sold,
                    total_revenue_cents: agg.total_revenue_cents,
                    total_discount_cents: agg.total_discount_cents,
                })
                .collect();

            if sort_by == "quantity" {
                products.sort_by_key(|b| std::cmp::Reverse(b.units_sold));
            } else {
                products.sort_by_key(|b| std::cmp::Reverse(b.total_revenue_cents));
            }
            products.truncate(limit as usize);
            Ok(products)
        }
    }
}
