//! Cashier performance and commission series.

use super::super::sales::{get_i64_flexible, recorded_cents};
use super::analytics_shared::{align_bucket_instant, date_match, invoices, trunc};

use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;
use crate::core::error::AppResult;
use crate::domain::reports::Granularity;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

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
                        entry.invoice_count += get_i64_flexible(row, "count")? as u64;
                        entry.revenue_cents += get_i64_flexible(row, "revenue")?;
                        entry.discount_given_cents += get_i64_flexible(row, "discount")?;
                        entry.credit_invoice_count += get_i64_flexible(row, "creditCount")? as u64;
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
                        entry.items_sold += get_i64_flexible(row, "units")?;
                        entry.retail_revenue_cents += get_i64_flexible(row, "retailRevenue")?;
                        entry.gross_profit_cents += get_i64_flexible(row, "retailRevenue")?
                            - get_i64_flexible(row, "cogs")?;
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
                entry.revenue_cents += get_i64_flexible(&doc, "total_cents")?;
                entry.discount_given_cents += get_i64_flexible(&doc, "discount_cents")?;
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
                            let qty = get_i64_flexible(item, "quantity")?;
                            let line_total = if item.contains_key("totalCents") {
                                get_i64_flexible(item, "totalCents")?
                            } else {
                                get_i64_flexible(item, "total_cents")?
                            };
                            let unit_cost = if item.contains_key("unitCostCents") {
                                recorded_cents(item, "unitCostCents")?
                            } else {
                                recorded_cents(item, "unit_cost_cents")?
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

/// `Σ earned commission` per `$dateTrunc` bucket, across delivered/completed
/// repairs + print jobs in `[start, end)`. Bucketed on the job's `created_at`
/// to match how revenue is recognised on the invoice date.
pub(crate) async fn aggregate_commission_series(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    granularity: Granularity,
) -> AppResult<std::collections::BTreeMap<i64, i64>> {
    match db {
        Db::Mongo(db) => {
            use crate::core::calculations::commission_cents;

            let unit = granularity.as_mongo_unit();
            let mut out: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();

            for collection in ["repairs", "print_jobs"] {
                let pipeline = vec![
                    doc! {
                        "$match": {
                            "deleted_at": null,
                            "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
                            "created_at": {
                                "$gte": BsonDateTime::from_chrono(start),
                                "$lt": BsonDateTime::from_chrono(end),
                            }
                        }
                    },
                    doc! {
                        "$project": {
                            "bucket": trunc(unit),
                            "estimated_cost_cents": 1,
                            "material_cost_cents": 1,
                            "split_type": 1,
                            "split_value": 1,
                        }
                    },
                ];

                let mut cursor = db
                    .collection::<Document>(collection)
                    .aggregate(pipeline)
                    .await?;
                while let Some(row) = cursor.try_next().await? {
                    let Some(ts) = row.get_datetime("bucket").ok().map(|d| d.to_chrono()) else {
                        continue;
                    };
                    let revenue = recorded_cents(&row, "estimated_cost_cents")?;
                    let material = recorded_cents(&row, "material_cost_cents")?;
                    let profit = crate::core::calculations::job_profit_cents(revenue, material);
                    let split_type = row.get_str("split_type").ok();
                    let split_value = row
                        .get_f64("split_value")
                        .ok()
                        .or_else(|| row.get_i64("split_value").ok().map(|v| v as f64))
                        .or_else(|| row.get_i32("split_value").ok().map(|v| v as f64))
                        .unwrap_or(0.0);
                    let earned = commission_cents(profit, split_type, split_value);
                    *out.entry(ts.timestamp_millis()).or_insert(0) += earned;
                }
            }

            Ok(out)
        }
        Db::Sqlite(pool) => {
            use crate::core::calculations::commission_cents;
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let mut out: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();

            for table in ["repairs", "print_jobs"] {
                let query_str = format!(
                    "SELECT created_at, estimated_cost_cents, material_cost_cents, split_type, split_value FROM {table} WHERE deleted_at IS NULL AND status IN ('completed', 'delivered', 'ready_for_pickup') AND created_at >= ? AND created_at < ?"
                );
                let rows = sqlx::query(&query_str)
                    .bind(&start_iso)
                    .bind(&end_iso)
                    .fetch_all(pool)
                    .await?;

                for row in &rows {
                    let doc = map_sqlite_row_to_document(row);
                    let created_at = if let Ok(bdt) = doc.get_datetime("created_at") {
                        bdt.to_chrono()
                    } else {
                        continue;
                    };
                    let bucket_start = align_bucket_instant(created_at, granularity);
                    let revenue = recorded_cents(&doc, "estimated_cost_cents")?;
                    let material = recorded_cents(&doc, "material_cost_cents")?;
                    let profit = crate::core::calculations::job_profit_cents(revenue, material);
                    let split_type = doc.get_str("split_type").ok();
                    let split_value = doc
                        .get_f64("split_value")
                        .ok()
                        .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
                        .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
                        .unwrap_or(0.0);
                    let earned = commission_cents(profit, split_type, split_value);
                    *out.entry(bucket_start.timestamp_millis()).or_insert(0) += earned;
                }
            }

            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// cashier performance
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct CashierAggRow {
    pub cashier_id: String,
    pub cashier_name: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub items_sold: i64,
    pub discount_given_cents: i64,
    pub credit_invoice_count: u64,
    pub gross_profit_cents: i64,
}
