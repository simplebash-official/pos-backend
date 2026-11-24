//! Revenue series, invoice KPIs and payment-method totals.

use super::super::sales;
use super::super::sales::get_i64_flexible;
use super::analytics_shared::{align_bucket_instant, date_match, invoices, trunc};

use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;
use crate::core::error::AppResult;
use crate::domain::reports::Granularity;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

/// Invoice-side KPIs for `[start, end)`: totals, per-stream revenue, retail
/// COGS from the per-line cost snapshot, and the coverage numerator.
pub(crate) async fn aggregate_invoice_kpis(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<InvoiceKpiAggregate> {
    match db {
        Db::Mongo(db) => {
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "gross": { "$sum": "$subtotal_cents" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "net": { "$sum": "$total_cents" },
                                }
                            }
                        ],
                        "streams": [
                            { "$unwind": "$items" },
                            {
                                "$group": {
                                    "_id": source_type.clone(),
                                    "revenue": { "$sum": line_total.clone() },
                                }
                            }
                        ],
                        "retail": [
                            { "$unwind": "$items" },
                            { "$match": { "$expr": { "$eq": [source_type.clone(), "retail"] } } },
                            {
                                "$group": {
                                    "_id": null,
                                    "units": { "$sum": { "$ifNull": ["$items.quantity", 0] } },
                                    "cogs": {
                                        "$sum": {
                                            "$multiply": [
                                                { "$ifNull": [unit_cost.clone(), 0] },
                                                { "$ifNull": ["$items.quantity", 0] },
                                            ]
                                        }
                                    },
                                    "revenueWithCost": {
                                        "$sum": {
                                            "$cond": [
                                                { "$ne": [unit_cost.clone(), null] },
                                                line_total.clone(),
                                                0,
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
            let mut agg = InvoiceKpiAggregate::default();

            if let Some(result) = cursor.try_next().await? {
                if let Ok(totals) = result.get_array("totals")
                    && let Some(first) = totals.first().and_then(|v| v.as_document())
                {
                    agg.invoice_count = get_i64_flexible(first, "count")? as u64;
                    agg.gross_sales_cents = get_i64_flexible(first, "gross")?;
                    agg.discount_cents = get_i64_flexible(first, "discount")?;
                    agg.net_sales_cents = get_i64_flexible(first, "net")?;
                }

                if let Ok(streams) = result.get_array("streams") {
                    for stream in streams.iter().filter_map(|s| s.as_document()) {
                        let revenue = get_i64_flexible(stream, "revenue")?;
                        match stream.get_str("_id").unwrap_or("") {
                            "retail" => agg.retail_revenue_cents += revenue,
                            "repair" => agg.repair_revenue_cents += revenue,
                            "print" => agg.print_revenue_cents += revenue,
                            "charge" => agg.charge_revenue_cents += revenue,
                            _ => {}
                        }
                    }
                }

                if let Ok(retail) = result.get_array("retail")
                    && let Some(first) = retail.first().and_then(|v| v.as_document())
                {
                    agg.retail_items_sold = get_i64_flexible(first, "units")?;
                    agg.retail_cogs_cents = get_i64_flexible(first, "cogs")?;
                    agg.retail_revenue_with_cost_cents =
                        get_i64_flexible(first, "revenueWithCost")?;
                }
            }

            Ok(agg)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT subtotal_cents, discount_cents, total_cents, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = InvoiceKpiAggregate::default();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                agg.invoice_count += 1;
                agg.gross_sales_cents += get_i64_flexible(&doc, "subtotal_cents")?;
                agg.discount_cents += get_i64_flexible(&doc, "discount_cents")?;
                agg.net_sales_cents += get_i64_flexible(&doc, "total_cents")?;

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        let line_total = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")?
                        } else {
                            get_i64_flexible(item, "total_cents")?
                        };
                        let unit_cost = if item.contains_key("unitCostCents") {
                            item.get_i64("unitCostCents").ok()
                        } else {
                            item.get_i64("unit_cost_cents").ok()
                        };
                        let qty = get_i64_flexible(item, "quantity")?;

                        match source_type {
                            "retail" => {
                                agg.retail_revenue_cents += line_total;
                                agg.retail_items_sold += qty;
                                if let Some(cost) = unit_cost {
                                    agg.retail_cogs_cents += cost * qty;
                                    agg.retail_revenue_with_cost_cents += line_total;
                                }
                            }
                            "repair" => agg.repair_revenue_cents += line_total,
                            "print" => agg.print_revenue_cents += line_total,
                            "charge" => agg.charge_revenue_cents += line_total,
                            _ => {}
                        }
                    }
                }
            }
            Ok(agg)
        }
    }
}

// ---------------------------------------------------------------------------
// payment methods (range-scoped)
// ---------------------------------------------------------------------------
pub(crate) async fn aggregate_payment_methods(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<(crate::domain::reports::PaymentMethodBreakdown, i64)> {
    let breakdown = sales::compute_payment_methods(db, start, end).await?;
    let total = breakdown.cash_cents
        + breakdown.card_cents
        + breakdown.online_cents
        + breakdown.credit_cents;
    Ok((breakdown, total))
}

/// Revenue / retail-COGS / discount / invoice-count per `$dateTrunc` bucket.
/// Sparse — empty buckets are absent and the service layer gap-fills them.
pub(crate) async fn aggregate_revenue_series(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    granularity: Granularity,
) -> AppResult<Vec<RawSeriesBucket>> {
    match db {
        Db::Mongo(db) => {
            let unit = granularity.as_mongo_unit();
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! { "$addFields": { "bucket": trunc(unit) } },
                doc! {
                    "$facet": {
                        "invoiceBuckets": [
                            {
                                "$group": {
                                    "_id": "$bucket",
                                    "count": { "$sum": 1 },
                                    "discount": { "$sum": "$discount_cents" },
                                }
                            }
                        ],
                        "streamBuckets": [
                            { "$unwind": "$items" },
                            {
                                "$group": {
                                    "_id": { "bucket": "$bucket", "sourceType": source_type.clone() },
                                    "revenue": { "$sum": line_total.clone() },
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
            let mut map: std::collections::BTreeMap<i64, RawSeriesBucket> =
                std::collections::BTreeMap::new();

            let bucket_instant = |doc: &Document, key: &str| -> Option<DateTime<Utc>> {
                doc.get_datetime(key).ok().map(|d| d.to_chrono())
            };

            if let Some(result) = cursor.try_next().await? {
                if let Ok(rows) = result.get_array("invoiceBuckets") {
                    for row in rows.iter().filter_map(|r| r.as_document()) {
                        let Some(ts) = bucket_instant(row, "_id") else {
                            continue;
                        };
                        let entry =
                            map.entry(ts.timestamp_millis())
                                .or_insert_with(|| RawSeriesBucket {
                                    period_start: ts,
                                    ..Default::default()
                                });
                        entry.invoice_count += get_i64_flexible(row, "count")? as u64;
                        entry.discount_cents += get_i64_flexible(row, "discount")?;
                    }
                }

                if let Ok(rows) = result.get_array("streamBuckets") {
                    for row in rows.iter().filter_map(|r| r.as_document()) {
                        let Ok(id) = row.get_document("_id") else {
                            continue;
                        };
                        let Some(ts) = bucket_instant(id, "bucket") else {
                            continue;
                        };
                        let revenue = get_i64_flexible(row, "revenue")?;
                        let cogs = get_i64_flexible(row, "cogs")?;
                        let entry =
                            map.entry(ts.timestamp_millis())
                                .or_insert_with(|| RawSeriesBucket {
                                    period_start: ts,
                                    ..Default::default()
                                });
                        entry.cogs_cents += cogs;
                        match id.get_str("sourceType").unwrap_or("") {
                            "retail" => entry.retail_revenue_cents += revenue,
                            "repair" => entry.repair_revenue_cents += revenue,
                            "print" => entry.print_revenue_cents += revenue,
                            "charge" => entry.charge_revenue_cents += revenue,
                            _ => {}
                        }
                    }
                }
            }

            Ok(map.into_values().collect())
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT created_at, discount_cents, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::BTreeMap<DateTime<Utc>, RawSeriesBucket> =
                std::collections::BTreeMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let created_at = if let Ok(bdt) = doc.get_datetime("created_at") {
                    bdt.to_chrono()
                } else {
                    continue;
                };
                let bucket_start = align_bucket_instant(created_at, granularity);
                let entry = map.entry(bucket_start).or_insert_with(|| RawSeriesBucket {
                    period_start: bucket_start,
                    ..Default::default()
                });
                entry.invoice_count += 1;
                entry.discount_cents += get_i64_flexible(&doc, "discount_cents")?;

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        let line_total = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")?
                        } else {
                            get_i64_flexible(item, "total_cents")?
                        };
                        let unit_cost = if item.contains_key("unitCostCents") {
                            item.get_i64("unitCostCents").ok()
                        } else {
                            item.get_i64("unit_cost_cents").ok()
                        };
                        let qty = get_i64_flexible(item, "quantity")?;

                        match source_type {
                            "retail" => {
                                entry.retail_revenue_cents += line_total;
                                if let Some(cost) = unit_cost {
                                    entry.cogs_cents += cost * qty;
                                }
                            }
                            "repair" => entry.repair_revenue_cents += line_total,
                            "print" => entry.print_revenue_cents += line_total,
                            "charge" => entry.charge_revenue_cents += line_total,
                            _ => {}
                        }
                    }
                }
            }

            Ok(map.into_values().collect())
        }
    }
}

// ---------------------------------------------------------------------------
// KPI aggregate (one facet over `invoices`)
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct InvoiceKpiAggregate {
    pub invoice_count: u64,
    pub gross_sales_cents: i64,
    pub discount_cents: i64,
    pub net_sales_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub charge_revenue_cents: i64,
    pub retail_items_sold: i64,
    pub retail_cogs_cents: i64,
    /// Retail revenue on lines that carried a `unitCostCents` snapshot.
    pub retail_revenue_with_cost_cents: i64,
}

// ---------------------------------------------------------------------------
// Time series
// ---------------------------------------------------------------------------

/// One raw time bucket straight from Mongo, keyed by its start instant.
#[derive(Debug, Clone, Default)]
pub(crate) struct RawSeriesBucket {
    pub period_start: DateTime<Utc>,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub charge_revenue_cents: i64,
    pub cogs_cents: i64,
    pub discount_cents: i64,
    pub invoice_count: u64,
}
