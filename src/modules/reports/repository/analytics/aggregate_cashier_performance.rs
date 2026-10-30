use super::super::sales::get_i64_flexible;
use super::cashier_agg_row::CashierAggRow;
use super::date_match::date_match;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::doc;

pub(crate) async fn aggregate_cashier_performance(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<CashierAggRow>> {
    match db {
        Db::Mongo(db) => {
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$facet": {
                        "invoiceLevel": [
                            {
                                "$group": {
                                    "_id": { "id": "$cashier_id", "name": "$cashier_name_snapshot" },
                                    "count": { "$sum": 1 },
                                    "revenue": { "$sum": "$total_cents" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "creditCount": {
                                        "$sum": { "$cond": [{ "$eq": ["$is_credit", true] }, 1, 0] }
                                    },
                                }
                            }
                        ],
                        "lineLevel": [
                            { "$addFields": { "cid": "$cashier_id" } },
                            { "$unwind": "$items" },
                            { "$match": { "$expr": { "$eq": [source_type.clone(), "retail"] } } },
                            {
                                "$group": {
                                    "_id": "$cid",
                                    "units": { "$sum": { "$ifNull": ["$items.quantity", 0] } },
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
            let mut rows: std::collections::HashMap<String, CashierAggRow> =
                std::collections::HashMap::new();

            if let Some(result) = cursor.try_next().await? {
                if let Ok(list) = result.get_array("invoiceLevel") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let Ok(id) = row.get_document("_id") else {
                            continue;
                        };
                        let cid = id.get_str("id").unwrap_or("").to_string();
                        if cid.is_empty() {
                            continue;
                        }
                        let entry = rows.entry(cid.clone()).or_default();
                        entry.cashier_id = cid;
                        entry.cashier_name = id.get_str("name").unwrap_or("Unknown").to_string();
                        entry.invoice_count += get_i64_flexible(row, "count") as u64;
                        entry.revenue_cents += get_i64_flexible(row, "revenue");
                        entry.discount_given_cents += get_i64_flexible(row, "discount");
                        entry.credit_invoice_count += get_i64_flexible(row, "creditCount") as u64;
                    }
                }
                if let Ok(list) = result.get_array("lineLevel") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let cid = row.get_str("_id").unwrap_or("").to_string();
                        if cid.is_empty() {
                            continue;
                        }
                        let entry = rows.entry(cid.clone()).or_default();
                        entry.cashier_id = cid;
                        entry.items_sold += get_i64_flexible(row, "units");
                        entry.retail_revenue_cents += get_i64_flexible(row, "retailRevenue");
                        entry.gross_profit_cents +=
                            get_i64_flexible(row, "retailRevenue") - get_i64_flexible(row, "cogs");
                    }
                }
            }

            Ok(rows.into_values().collect())
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT cashier_id, cashier_name_snapshot, total_cents, discount_cents, is_credit, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::HashMap<String, CashierAggRow> =
                std::collections::HashMap::new();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let cid = doc.get_str("cashier_id").unwrap_or("").to_string();
                if cid.is_empty() {
                    continue;
                }
                let name = doc
                    .get_str("cashier_name_snapshot")
                    .unwrap_or("Unknown")
                    .to_string();
                let entry = map.entry(cid.clone()).or_default();
                entry.cashier_id = cid;
                if entry.cashier_name.is_empty() || entry.cashier_name == "Unknown" {
                    entry.cashier_name = name;
                }
                entry.invoice_count += 1;
                entry.revenue_cents += get_i64_flexible(&doc, "total_cents");
                entry.discount_given_cents += get_i64_flexible(&doc, "discount_cents");
                if doc.get_bool("is_credit").unwrap_or(false) {
                    entry.credit_invoice_count += 1;
                }

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        if source_type == "retail" {
                            let qty = get_i64_flexible(item, "quantity");
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
                            entry.items_sold += qty;
                            entry.retail_revenue_cents += line_total;
                            entry.gross_profit_cents += line_total - (unit_cost * qty);
                        }
                    }
                }
            }
            Ok(map.into_values().collect())
        }
    }
}
