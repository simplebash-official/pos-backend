use super::super::sales::get_i64_flexible;
use super::date_match::date_match;
use super::invoices::invoices;
use super::top_customer_agg_row::TopCustomerAggRow;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Per-customer sales over `[start, end)`. Walk-in invoices (no `customer_key`)
/// are returned as one row with `customer_key = None`.
pub(crate) async fn aggregate_top_customers(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<TopCustomerAggRow>> {
    match db {
        Db::Mongo(db) => {
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };
            let cust = doc! { "$ifNull": ["$customer_key", null] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$facet": {
                        "byCustomer": [
                            {
                                "$group": {
                                    "_id": {
                                        "key": cust.clone(),
                                        "name": { "$ifNull": ["$customer_name_snapshot", "Walk-in"] },
                                        "phone": "$customer_phone_snapshot",
                                    },
                                    "count": { "$sum": 1 },
                                    "revenue": { "$sum": "$total_cents" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "last": { "$max": "$created_at" },
                                }
                            }
                        ],
                        "retailByCustomer": [
                            { "$addFields": { "custKey": cust.clone() } },
                            { "$unwind": "$items" },
                            { "$match": { "$expr": { "$eq": [source_type.clone(), "retail"] } } },
                            {
                                "$group": {
                                    "_id": "$custKey",
                                    "retailRevenue": { "$sum": line_total.clone() },
                                    "cogs": {
                                        "$sum": {
                                            "$multiply": [
                                                { "$ifNull": [unit_cost.clone(), 0] },
                                                { "$ifNull": ["$items.quantity", 0] },
                                            ]
                                        }
                                    },
                                }
                            }
                        ],
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut rows: std::collections::HashMap<Option<String>, TopCustomerAggRow> =
                std::collections::HashMap::new();

            if let Some(result) = cursor.try_next().await? {
                if let Ok(list) = result.get_array("byCustomer") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let Ok(id) = row.get_document("_id") else {
                            continue;
                        };
                        let key = id.get_str("key").ok().map(String::from);
                        let entry = rows.entry(key.clone()).or_default();
                        if entry.customer_name.is_empty() {
                            entry.customer_key = key;
                            entry.customer_name =
                                id.get_str("name").unwrap_or("Walk-in").to_string();
                            entry.customer_phone = id.get_str("phone").ok().map(String::from);
                        }
                        entry.invoice_count += get_i64_flexible(row, "count") as u64;
                        entry.revenue_cents += get_i64_flexible(row, "revenue");
                        entry.discount_cents += get_i64_flexible(row, "discount");
                        if let Ok(dt) = row.get_datetime("last") {
                            let ts = dt.to_chrono();
                            if ts > entry.last_purchase_at {
                                entry.last_purchase_at = ts;
                            }
                        }
                    }
                }
                if let Ok(list) = result.get_array("retailByCustomer") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let key = row.get_str("_id").ok().map(String::from);
                        let profit =
                            get_i64_flexible(row, "retailRevenue") - get_i64_flexible(row, "cogs");
                        rows.entry(key).or_default().gross_profit_cents += profit;
                    }
                }
            }

            Ok(rows.into_values().collect())
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT customer_key, customer_name_snapshot, customer_phone_snapshot, total_cents, discount_cents, created_at, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::HashMap<Option<String>, TopCustomerAggRow> =
                std::collections::HashMap::new();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let key = doc
                    .get_str("customer_key")
                    .ok()
                    .map(String::from)
                    .filter(|s| !s.is_empty());
                let entry = map.entry(key.clone()).or_default();
                if entry.customer_name.is_empty() {
                    entry.customer_key = key;
                    entry.customer_name = doc
                        .get_str("customer_name_snapshot")
                        .unwrap_or("Walk-in")
                        .to_string();
                    entry.customer_phone = doc
                        .get_str("customer_phone_snapshot")
                        .ok()
                        .map(String::from);
                }
                entry.invoice_count += 1;
                entry.revenue_cents += get_i64_flexible(&doc, "total_cents");
                entry.discount_cents += get_i64_flexible(&doc, "discount_cents");
                if let Ok(bdt) = doc.get_datetime("created_at") {
                    let ts = bdt.to_chrono();
                    if ts > entry.last_purchase_at {
                        entry.last_purchase_at = ts;
                    }
                }

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        if source_type == "retail" {
                            let line_total = if item.contains_key("totalCents") {
                                get_i64_flexible(item, "totalCents")
                            } else {
                                get_i64_flexible(item, "total_cents")
                            };
                            let unit_cost = if item.contains_key("unitCostCents") {
                                get_i64_flexible(item, "unitCostCents")
                            } else {
                                get_i64_flexible(item, "unit_cost_cents")
                            };
                            let qty = get_i64_flexible(item, "quantity");
                            let cogs = unit_cost * qty;
                            entry.gross_profit_cents += line_total - cogs;
                        }
                    }
                }
            }
            Ok(map.into_values().collect())
        }
    }
}
