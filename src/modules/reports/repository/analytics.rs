// MongoDB aggregation for the Analytics & Reports endpoints (`/reports/analytics/*`).
//
// All pipelines `$match` `invoices` on `{ status: { $ne: "voided" }, created_at
// in [start, end) }` and, where a line-level figure is needed, `$unwind
// "$items"` while `$ifNull`-coalescing the two historical field spellings
// (`sourceType`/`source_type`, `totalCents`/`total_cents`,
// `unitCostCents`/`unit_cost_cents`, …) exactly as `repository::sales` does.
//
// Time buckets use `$dateTrunc` with `timezone: "+05:30"` (Sri Lanka is
// permanently UTC+05:30) and `startOfWeek: "monday"` so a bucket lines up
// with the shop owner's local calendar — see
// `service::dates::{REPORT_TZ_MONGO, parse_date_range_tz}`.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

use crate::{
    clients::{db::Db, sqlite::map_sqlite_row_to_document},
    core::error::AppResult,
    domain::reports::Granularity,
};

use super::super::service::dates::REPORT_TZ_MONGO;
use super::sales::{self, get_i64_flexible};

fn invoices(db: &TenantDatabase) -> ScopedCollection<Document> {
    db.collection("invoices")
}

fn align_bucket_instant(instant: DateTime<Utc>, gran: Granularity) -> DateTime<Utc> {
    use chrono::{Datelike, Duration, FixedOffset, NaiveDate};
    let tz = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
    let local = instant.with_timezone(&tz);
    let date = local.date_naive();
    let aligned_date = match gran {
        Granularity::Day => date,
        Granularity::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
        Granularity::Month => NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap(),
        Granularity::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap(),
    };
    aligned_date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(tz)
        .single()
        .unwrap()
        .with_timezone(&Utc)
}

fn date_match(start: DateTime<Utc>, end: DateTime<Utc>) -> Document {
    doc! {
        "status": { "$ne": "voided" },
        "created_at": {
            "$gte": BsonDateTime::from_chrono(start),
            "$lt": BsonDateTime::from_chrono(end),
        }
    }
}

fn trunc(unit: &str) -> Document {
    doc! {
        "$dateTrunc": {
            "date": "$created_at",
            "unit": unit,
            "binSize": 1,
            "timezone": REPORT_TZ_MONGO,
            "startOfWeek": "monday",
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
    pub retail_items_sold: i64,
    pub retail_cogs_cents: i64,
    /// Retail revenue on lines that carried a `unitCostCents` snapshot.
    pub retail_revenue_with_cost_cents: i64,
}

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
                    agg.invoice_count = get_i64_flexible(first, "count") as u64;
                    agg.gross_sales_cents = get_i64_flexible(first, "gross");
                    agg.discount_cents = get_i64_flexible(first, "discount");
                    agg.net_sales_cents = get_i64_flexible(first, "net");
                }

                if let Ok(streams) = result.get_array("streams") {
                    for stream in streams.iter().filter_map(|s| s.as_document()) {
                        let revenue = get_i64_flexible(stream, "revenue");
                        match stream.get_str("_id").unwrap_or("") {
                            "retail" => agg.retail_revenue_cents += revenue,
                            "repair" => agg.repair_revenue_cents += revenue,
                            "print" => agg.print_revenue_cents += revenue,
                            _ => {}
                        }
                    }
                }

                if let Ok(retail) = result.get_array("retail")
                    && let Some(first) = retail.first().and_then(|v| v.as_document())
                {
                    agg.retail_items_sold = get_i64_flexible(first, "units");
                    agg.retail_cogs_cents = get_i64_flexible(first, "cogs");
                    agg.retail_revenue_with_cost_cents = get_i64_flexible(first, "revenueWithCost");
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
                agg.gross_sales_cents += get_i64_flexible(&doc, "subtotal_cents");
                agg.discount_cents += get_i64_flexible(&doc, "discount_cents");
                agg.net_sales_cents += get_i64_flexible(&doc, "total_cents");

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        let line_total = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")
                        } else {
                            get_i64_flexible(item, "total_cents")
                        };
                        let unit_cost = if item.contains_key("unitCostCents") {
                            item.get_i64("unitCostCents").ok()
                        } else {
                            item.get_i64("unit_cost_cents").ok()
                        };
                        let qty = get_i64_flexible(item, "quantity");

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
// Time series
// ---------------------------------------------------------------------------

/// One raw time bucket straight from Mongo, keyed by its start instant.
#[derive(Debug, Clone, Default)]
pub(crate) struct RawSeriesBucket {
    pub period_start: DateTime<Utc>,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub cogs_cents: i64,
    pub discount_cents: i64,
    pub invoice_count: u64,
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
                        entry.invoice_count += get_i64_flexible(row, "count") as u64;
                        entry.discount_cents += get_i64_flexible(row, "discount");
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
                        let revenue = get_i64_flexible(row, "revenue");
                        let cogs = get_i64_flexible(row, "cogs");
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
                entry.discount_cents += get_i64_flexible(&doc, "discount_cents");

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        let line_total = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")
                        } else {
                            get_i64_flexible(item, "total_cents")
                        };
                        let unit_cost = if item.contains_key("unitCostCents") {
                            item.get_i64("unitCostCents").ok()
                        } else {
                            item.get_i64("unit_cost_cents").ok()
                        };
                        let qty = get_i64_flexible(item, "quantity");

                        match source_type {
                            "retail" => {
                                entry.retail_revenue_cents += line_total;
                                if let Some(cost) = unit_cost {
                                    entry.cogs_cents += cost * qty;
                                }
                            }
                            "repair" => entry.repair_revenue_cents += line_total,
                            "print" => entry.print_revenue_cents += line_total,
                            _ => {}
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
            use crate::modules::reports::repository::commissions::compute_earned_cents;

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
                    let revenue = get_i64_flexible(&row, "estimated_cost_cents");
                    let material = get_i64_flexible(&row, "material_cost_cents");
                    let profit = (revenue - material).max(0);
                    let split_type = row.get_str("split_type").ok();
                    let split_value = row
                        .get_f64("split_value")
                        .ok()
                        .or_else(|| row.get_i64("split_value").ok().map(|v| v as f64))
                        .or_else(|| row.get_i32("split_value").ok().map(|v| v as f64))
                        .unwrap_or(0.0);
                    let earned = compute_earned_cents(profit, split_type, split_value);
                    *out.entry(ts.timestamp_millis()).or_insert(0) += earned;
                }
            }

            Ok(out)
        }
        Db::Sqlite(pool) => {
            use crate::modules::reports::repository::commissions::compute_earned_cents;
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
                    let revenue = get_i64_flexible(&doc, "estimated_cost_cents");
                    let material = get_i64_flexible(&doc, "material_cost_cents");
                    let profit = (revenue - material).max(0);
                    let split_type = doc.get_str("split_type").ok();
                    let split_value = doc
                        .get_f64("split_value")
                        .ok()
                        .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
                        .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
                        .unwrap_or(0.0);
                    let earned = compute_earned_cents(profit, split_type, split_value);
                    *out.entry(bucket_start.timestamp_millis()).or_insert(0) += earned;
                }
            }

            Ok(out)
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

// ---------------------------------------------------------------------------
// top customers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct TopCustomerAggRow {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    /// retail line revenue − retail COGS for this customer (product margin only).
    pub gross_profit_cents: i64,
    pub last_purchase_at: DateTime<Utc>,
}

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

/// `customer.key -> outstanding_balance_cents` for the given keys.
pub(crate) async fn fetch_customer_balances(
    db: &Db,
    keys: &[String],
) -> AppResult<std::collections::HashMap<String, i64>> {
    match db {
        Db::Mongo(db) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            let mut cursor = db
                .collection::<Document>("customers")
                .find(doc! { "key": { "$in": keys } })
                .projection(doc! { "key": 1, "outstanding_balance_cents": 1 })
                .await?;
            while let Some(row) = cursor.try_next().await? {
                if let Ok(k) = row.get_str("key") {
                    out.insert(
                        k.to_string(),
                        get_i64_flexible(&row, "outstanding_balance_cents"),
                    );
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            use sqlx::Row;
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT key, outstanding_balance_cents FROM customers WHERE key IN ({placeholders})"
            );
            let mut q = sqlx::query(&sql);
            for k in keys {
                q = q.bind(k);
            }
            let rows = q.fetch_all(pool).await?;
            for r in rows {
                let k: String = r.get("key");
                let bal: i64 = r.try_get("outstanding_balance_cents").unwrap_or(0);
                out.insert(k, bal);
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// sales by category
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct CategoryAggRow {
    pub category_key: Option<String>,
    pub category_name: String,
    pub subcategory_key: Option<String>,
    pub subcategory_name: Option<String>,
    pub units_sold: i64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    pub cogs_cents: i64,
}

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

/// `cashier_id -> (refund credit-note count, Σ net_refund_cents)` over
/// non-voided credit notes in `[start, end)`.
pub(crate) async fn aggregate_cashier_refunds(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<std::collections::HashMap<String, (u64, i64)>> {
    match db {
        Db::Mongo(db) => {
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
                doc! {
                    "$group": {
                        "_id": "$cashier_id",
                        "count": { "$sum": 1 },
                        "amount": { "$sum": "$net_refund_cents" },
                    }
                },
            ];
            let mut cursor = db
                .collection::<Document>("credit_notes")
                .aggregate(pipeline)
                .await?;
            let mut out = std::collections::HashMap::new();
            while let Some(row) = cursor.try_next().await? {
                if let Ok(cid) = row.get_str("_id") {
                    out.insert(
                        cid.to_string(),
                        (
                            get_i64_flexible(&row, "count") as u64,
                            get_i64_flexible(&row, "amount"),
                        ),
                    );
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use sqlx::Row;
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT cashier_id, COUNT(*) as cnt, COALESCE(SUM(net_refund_cents), 0) as amount FROM credit_notes WHERE status != 'voided' AND created_at >= ? AND created_at < ? GROUP BY cashier_id"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut out = std::collections::HashMap::new();
            for r in rows {
                let cid: Option<String> = r.get("cashier_id");
                if let Some(cid) = cid {
                    let cnt: i64 = r.get("cnt");
                    let amt: i64 = r.get("amount");
                    out.insert(cid, (cnt as u64, amt));
                }
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// sales patterns (weekday x hour)
// ---------------------------------------------------------------------------

/// `(weekday 1..7, hour 0..23, invoice count, revenue)` cells present in the
/// data — the service layer zero-fills the full 7×24 grid.
pub(crate) async fn aggregate_sales_patterns(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<(u8, u8, u64, i64)>> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$group": {
                        "_id": {
                            "weekday": { "$isoDayOfWeek": { "date": "$created_at", "timezone": REPORT_TZ_MONGO } },
                            "hour": { "$hour": { "date": "$created_at", "timezone": REPORT_TZ_MONGO } },
                        },
                        "count": { "$sum": 1 },
                        "revenue": { "$sum": "$total_cents" },
                    }
                },
            ];
            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let Ok(id) = row.get_document("_id") else {
                    continue;
                };
                let weekday = get_i64_flexible(id, "weekday").clamp(1, 7) as u8;
                let hour = get_i64_flexible(id, "hour").clamp(0, 23) as u8;
                out.push((
                    weekday,
                    hour,
                    get_i64_flexible(&row, "count") as u64,
                    get_i64_flexible(&row, "revenue"),
                ));
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use chrono::{Datelike, FixedOffset, Timelike};
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT created_at, total_cents FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let tz = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
            let mut map: std::collections::HashMap<(u8, u8), (u64, i64)> =
                std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let created_at = if let Ok(bdt) = doc.get_datetime("created_at") {
                    bdt.to_chrono()
                } else {
                    continue;
                };
                let local = created_at.with_timezone(&tz);
                let weekday = local.weekday().number_from_monday() as u8;
                let hour = local.hour() as u8;
                let rev = get_i64_flexible(&doc, "total_cents");
                let entry = map.entry((weekday, hour)).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += rev;
            }

            let out = map
                .into_iter()
                .map(|((w, h), (cnt, rev))| (w, h, cnt, rev))
                .collect();
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// receivables aging
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct AgingInvoiceRow {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub balance_due_cents: i64,
    pub days_overdue: i32,
}

/// Every open credit invoice (`pending`/`partially_paid`) with its current
/// balance and age in days relative to `as_of`. The service layer buckets
/// these and derives the top-debtor list.
pub(crate) async fn aggregate_receivables_aging(
    db: &Db,
    as_of: DateTime<Utc>,
) -> AppResult<Vec<AgingInvoiceRow>> {
    match db {
        Db::Mongo(db) => {
            let as_of_bson = BsonDateTime::from_chrono(as_of);
            let pipeline = vec![
                doc! { "$match": { "status": { "$in": ["pending", "partially_paid"] } } },
                doc! {
                    "$lookup": {
                        "from": "payments",
                        "localField": "key",
                        "foreignField": "invoice_key",
                        "as": "pd",
                    }
                },
                doc! {
                    "$project": {
                        "customer_key": 1,
                        "customer_name_snapshot": 1,
                        "customer_phone_snapshot": 1,
                        "due_date": 1,
                        "created_at": 1,
                        "balance_due": {
                            "$max": [0, { "$subtract": ["$total_cents", { "$sum": "$pd.amount_cents" }] }]
                        },
                    }
                },
                doc! { "$match": { "balance_due": { "$gt": 0 } } },
                doc! {
                    "$addFields": {
                        "anchor": {
                            "$dateFromString": {
                                "dateString": { "$ifNull": ["$due_date", null] },
                                "onError": "$created_at",
                                "onNull": "$created_at",
                            }
                        }
                    }
                },
                doc! {
                    "$addFields": {
                        "days": { "$dateDiff": { "startDate": "$anchor", "endDate": as_of_bson, "unit": "day" } }
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                out.push(AgingInvoiceRow {
                    customer_key: row.get_str("customer_key").ok().map(String::from),
                    customer_name: row
                        .get_str("customer_name_snapshot")
                        .unwrap_or("Unknown")
                        .to_string(),
                    customer_phone: row
                        .get_str("customer_phone_snapshot")
                        .ok()
                        .map(String::from),
                    balance_due_cents: get_i64_flexible(&row, "balance_due"),
                    days_overdue: get_i64_flexible(&row, "days") as i32,
                });
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use sqlx::Row;
            let rows = sqlx::query(
                "SELECT i.customer_key, i.customer_name_snapshot, i.customer_phone_snapshot, i.due_date, i.created_at, i.total_cents, COALESCE(SUM(p.amount_cents), 0) as paid_cents FROM invoices i LEFT JOIN payments p ON p.invoice_key = i.key WHERE i.status IN ('pending', 'partially_paid') GROUP BY i.key"
            )
            .fetch_all(pool)
            .await?;

            let mut out = Vec::new();
            for r in rows {
                let total_cents: i64 = r.get("total_cents");
                let paid_cents: i64 = r.get("paid_cents");
                let balance = (total_cents - paid_cents).max(0);
                if balance <= 0 {
                    continue;
                }

                let due_date_opt: Option<String> = r.get("due_date");
                let created_at_str: String = r.get("created_at");

                let anchor = due_date_opt
                    .as_deref()
                    .and_then(|d| {
                        DateTime::parse_from_rfc3339(d)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
                    .or_else(|| {
                        DateTime::parse_from_rfc3339(&created_at_str)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
                    .unwrap_or(as_of);

                let days = (as_of - anchor).num_days() as i32;

                out.push(AgingInvoiceRow {
                    customer_key: r.get("customer_key"),
                    customer_name: r
                        .try_get("customer_name_snapshot")
                        .unwrap_or_else(|_| "Unknown".to_string()),
                    customer_phone: r.get("customer_phone_snapshot"),
                    balance_due_cents: balance,
                    days_overdue: days,
                });
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// discounts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct DiscountAgg {
    pub gross_before_discount_cents: i64,
    pub order_level_discount_cents: i64,
    pub line_level_discount_cents: i64,
    pub invoice_count: u64,
    pub invoices_with_discount: u64,
    pub by_type: Vec<(String, u64, i64)>,
    pub by_cashier: Vec<(String, String, i64, i64)>,
    pub top_products: Vec<(Option<String>, String, i64, i64)>,
}

pub(crate) async fn aggregate_discounts(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<DiscountAgg> {
    match db {
        Db::Mongo(db) => {
            let line_discount =
                doc! { "$ifNull": ["$items.discountCents", "$items.discount_cents"] };
            let product_key = doc! { "$ifNull": ["$items.productKey", "$items.product_key"] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "gross": { "$sum": "$subtotal_cents" },
                                    "orderDiscount": { "$sum": "$discount_cents" },
                                    "count": { "$sum": 1 },
                                    "withDiscount": {
                                        "$sum": { "$cond": [{ "$gt": ["$discount_cents", 0] }, 1, 0] }
                                    },
                                }
                            }
                        ],
                        "byType": [
                            {
                                "$group": {
                                    "_id": { "$ifNull": ["$discount_type", "none"] },
                                    "count": { "$sum": 1 },
                                    "discount": { "$sum": "$discount_cents" },
                                }
                            }
                        ],
                        "byCashier": [
                            {
                                "$group": {
                                    "_id": { "id": "$cashier_id", "name": "$cashier_name_snapshot" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "revenue": { "$sum": "$total_cents" },
                                }
                            }
                        ],
                        "lineTotals": [
                            { "$unwind": "$items" },
                            { "$group": { "_id": null, "lineDiscount": { "$sum": line_discount.clone() } } }
                        ],
                        "topProducts": [
                            { "$unwind": "$items" },
                            { "$match": { "$expr": { "$gt": [line_discount.clone(), 0] } } },
                            {
                                "$group": {
                                    "_id": { "key": product_key.clone(), "name": "$items.name" },
                                    "discount": { "$sum": line_discount.clone() },
                                    "units": { "$sum": { "$ifNull": ["$items.quantity", 0] } },
                                }
                            },
                            { "$sort": { "discount": -1 } },
                            { "$limit": 10 },
                        ],
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut agg = DiscountAgg::default();
            if let Some(result) = cursor.try_next().await? {
                if let Some(t) = result
                    .get_array("totals")
                    .ok()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_document())
                {
                    agg.gross_before_discount_cents = get_i64_flexible(t, "gross");
                    agg.order_level_discount_cents = get_i64_flexible(t, "orderDiscount");
                    agg.invoice_count = get_i64_flexible(t, "count") as u64;
                    agg.invoices_with_discount = get_i64_flexible(t, "withDiscount") as u64;
                }
                if let Some(l) = result
                    .get_array("lineTotals")
                    .ok()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_document())
                {
                    agg.line_level_discount_cents = get_i64_flexible(l, "lineDiscount");
                }
                if let Ok(list) = result.get_array("byType") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_type.push((
                            row.get_str("_id").unwrap_or("none").to_string(),
                            get_i64_flexible(row, "count") as u64,
                            get_i64_flexible(row, "discount"),
                        ));
                    }
                }
                if let Ok(list) = result.get_array("byCashier") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let id = row.get_document("_id").ok();
                        let cid = id.and_then(|d| d.get_str("id").ok()).unwrap_or("");
                        if cid.is_empty() {
                            continue;
                        }
                        agg.by_cashier.push((
                            cid.to_string(),
                            id.and_then(|d| d.get_str("name").ok())
                                .unwrap_or("Unknown")
                                .to_string(),
                            get_i64_flexible(row, "discount"),
                            get_i64_flexible(row, "revenue"),
                        ));
                    }
                }
                if let Ok(list) = result.get_array("topProducts") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let id = row.get_document("_id").ok();
                        agg.top_products.push((
                            id.and_then(|d| d.get_str("key").ok()).map(String::from),
                            id.and_then(|d| d.get_str("name").ok())
                                .unwrap_or("Unknown")
                                .to_string(),
                            get_i64_flexible(row, "discount"),
                            get_i64_flexible(row, "units"),
                        ));
                    }
                }
            }
            Ok(agg)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT subtotal_cents, discount_cents, discount_type, total_cents, cashier_id, cashier_name_snapshot, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = DiscountAgg::default();
            let mut by_type_map: std::collections::HashMap<String, (u64, i64)> =
                std::collections::HashMap::new();
            let mut by_cashier_map: std::collections::HashMap<String, (String, i64, i64)> =
                std::collections::HashMap::new();
            let mut top_products_map: std::collections::HashMap<
                (Option<String>, String),
                (i64, i64),
            > = std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let gross = get_i64_flexible(&doc, "subtotal_cents");
                let order_discount = get_i64_flexible(&doc, "discount_cents");
                let total = get_i64_flexible(&doc, "total_cents");
                let dtype = doc.get_str("discount_type").unwrap_or("none").to_string();
                let cid = doc.get_str("cashier_id").unwrap_or("").to_string();
                let cname = doc
                    .get_str("cashier_name_snapshot")
                    .unwrap_or("Unknown")
                    .to_string();

                agg.gross_before_discount_cents += gross;
                agg.order_level_discount_cents += order_discount;
                agg.invoice_count += 1;
                if order_discount > 0 {
                    agg.invoices_with_discount += 1;
                }

                let type_entry = by_type_map.entry(dtype).or_insert((0, 0));
                type_entry.0 += 1;
                type_entry.1 += order_discount;

                if !cid.is_empty() {
                    let c_entry = by_cashier_map.entry(cid.clone()).or_insert((cname, 0, 0));
                    c_entry.1 += order_discount;
                    c_entry.2 += total;
                }

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let l_discount = if item.contains_key("discountCents") {
                            get_i64_flexible(item, "discountCents")
                        } else {
                            get_i64_flexible(item, "discount_cents")
                        };
                        agg.line_level_discount_cents += l_discount;

                        if l_discount > 0 {
                            let pk = item
                                .get_str("productKey")
                                .or_else(|_| item.get_str("product_key"))
                                .ok()
                                .map(String::from);
                            let name = item.get_str("name").unwrap_or("Unknown").to_string();
                            let qty = get_i64_flexible(item, "quantity");
                            let p_entry = top_products_map.entry((pk, name)).or_insert((0, 0));
                            p_entry.0 += l_discount;
                            p_entry.1 += qty;
                        }
                    }
                }
            }

            agg.by_type = by_type_map
                .into_iter()
                .map(|(t, (c, d))| (t, c, d))
                .collect();
            agg.by_cashier = by_cashier_map
                .into_iter()
                .map(|(cid, (name, d, rev))| (cid, name, d, rev))
                .collect();
            let mut top_prods: Vec<_> = top_products_map
                .into_iter()
                .map(|((pk, name), (d, q))| (pk, name, d, q))
                .collect();
            top_prods.sort_by_key(|b| std::cmp::Reverse(b.2));
            top_prods.truncate(10);
            agg.top_products = top_prods;

            Ok(agg)
        }
    }
}

// ---------------------------------------------------------------------------
// refunds (detailed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct RefundDetailAgg {
    pub credit_note_count: u64,
    pub net_refund_cents: i64,
    pub refund_cash_cents: i64,
    pub balance_reduction_cents: i64,
    pub exchange_count: u64,
    pub no_receipt_count: u64,
    pub manager_override_count: u64,
    pub by_reason: Vec<(String, u64, i64)>,
    pub by_method: Vec<(String, i64)>,
    pub top_products: Vec<(Option<String>, String, i64, i64)>,
}

pub(crate) async fn aggregate_refund_detail(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<RefundDetailAgg> {
    match db {
        Db::Mongo(db) => {
            let item_total =
                doc! { "$ifNull": ["$returned_items.totalCents", "$returned_items.total_cents"] };
            let item_key =
                doc! { "$ifNull": ["$returned_items.productKey", "$returned_items.product_key"] };

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
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "net": { "$sum": "$net_refund_cents" },
                                    "cash": { "$sum": "$refund_cash_cents" },
                                    "balance": { "$sum": "$balance_reduction_cents" },
                                    "noReceipt": { "$sum": { "$cond": ["$no_receipt", 1, 0] } },
                                    "override": { "$sum": { "$cond": ["$is_manager_override", 1, 0] } },
                                    "exchange": {
                                        "$sum": {
                                            "$cond": [
                                                { "$gt": [{ "$size": { "$ifNull": ["$exchange_items", []] } }, 0] },
                                                1, 0
                                            ]
                                        }
                                    },
                                }
                            }
                        ],
                        "byMethod": [
                            { "$unwind": { "path": "$refund_breakdown", "preserveNullAndEmptyArrays": false } },
                            {
                                "$group": {
                                    "_id": "$refund_breakdown.method",
                                    "amount": { "$sum": "$refund_breakdown.amount_cents" },
                                }
                            }
                        ],
                        "byReason": [
                            { "$unwind": "$returned_items" },
                            {
                                "$group": {
                                    "_id": { "$ifNull": ["$returned_items.reason", "other"] },
                                    "count": { "$sum": 1 },
                                    "amount": { "$sum": item_total.clone() },
                                }
                            }
                        ],
                        "topProducts": [
                            { "$unwind": "$returned_items" },
                            {
                                "$group": {
                                    "_id": { "key": item_key.clone(), "name": "$returned_items.name" },
                                    "qty": { "$sum": { "$ifNull": ["$returned_items.quantity", 0] } },
                                    "amount": { "$sum": item_total.clone() },
                                }
                            },
                            { "$sort": { "amount": -1 } },
                            { "$limit": 10 },
                        ],
                    }
                },
            ];

            let mut cursor = db
                .collection::<Document>("credit_notes")
                .aggregate(pipeline)
                .await?;
            let mut agg = RefundDetailAgg::default();
            if let Some(result) = cursor.try_next().await? {
                if let Some(t) = result
                    .get_array("totals")
                    .ok()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_document())
                {
                    agg.credit_note_count = get_i64_flexible(t, "count") as u64;
                    agg.net_refund_cents = get_i64_flexible(t, "net");
                    agg.refund_cash_cents = get_i64_flexible(t, "cash");
                    agg.balance_reduction_cents = get_i64_flexible(t, "balance");
                    agg.no_receipt_count = get_i64_flexible(t, "noReceipt") as u64;
                    agg.manager_override_count = get_i64_flexible(t, "override") as u64;
                    agg.exchange_count = get_i64_flexible(t, "exchange") as u64;
                }
                if let Ok(list) = result.get_array("byMethod") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_method.push((
                            row.get_str("_id").unwrap_or("cash").to_string(),
                            get_i64_flexible(row, "amount"),
                        ));
                    }
                }
                if let Ok(list) = result.get_array("byReason") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_reason.push((
                            row.get_str("_id").unwrap_or("other").to_string(),
                            get_i64_flexible(row, "count") as u64,
                            get_i64_flexible(row, "amount"),
                        ));
                    }
                }
                if let Ok(list) = result.get_array("topProducts") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let id = row.get_document("_id").ok();
                        agg.top_products.push((
                            id.and_then(|d| d.get_str("key").ok()).map(String::from),
                            id.and_then(|d| d.get_str("name").ok())
                                .unwrap_or("Unknown")
                                .to_string(),
                            get_i64_flexible(row, "qty"),
                            get_i64_flexible(row, "amount"),
                        ));
                    }
                }
            }
            Ok(agg)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT * FROM credit_notes WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = RefundDetailAgg::default();
            let mut by_method_map: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            let mut by_reason_map: std::collections::HashMap<String, (u64, i64)> =
                std::collections::HashMap::new();
            let mut top_prods_map: std::collections::HashMap<(Option<String>, String), (i64, i64)> =
                std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                agg.credit_note_count += 1;
                agg.net_refund_cents += get_i64_flexible(&doc, "net_refund_cents");
                agg.refund_cash_cents += get_i64_flexible(&doc, "refund_cash_cents");
                agg.balance_reduction_cents += get_i64_flexible(&doc, "balance_reduction_cents");
                if doc.get_bool("no_receipt").unwrap_or(false) {
                    agg.no_receipt_count += 1;
                }
                if doc.get_bool("is_manager_override").unwrap_or(false) {
                    agg.manager_override_count += 1;
                }
                if let Ok(ex) = doc.get_array("exchange_items")
                    && !ex.is_empty()
                {
                    agg.exchange_count += 1;
                }

                if let Ok(bdown) = doc.get_array("refund_breakdown") {
                    for item in bdown.iter().filter_map(|i| i.as_document()) {
                        let m = item.get_str("method").unwrap_or("cash").to_string();
                        let amt = get_i64_flexible(item, "amount_cents");
                        *by_method_map.entry(m).or_insert(0) += amt;
                    }
                }

                if let Ok(ret_items) = doc.get_array("returned_items") {
                    for item in ret_items.iter().filter_map(|i| i.as_document()) {
                        let reason = item.get_str("reason").unwrap_or("other").to_string();
                        let amt = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")
                        } else {
                            get_i64_flexible(item, "total_cents")
                        };
                        let r_entry = by_reason_map.entry(reason).or_insert((0, 0));
                        r_entry.0 += 1;
                        r_entry.1 += amt;

                        let pk = item
                            .get_str("productKey")
                            .or_else(|_| item.get_str("product_key"))
                            .ok()
                            .map(String::from);
                        let name = item.get_str("name").unwrap_or("Unknown").to_string();
                        let qty = get_i64_flexible(item, "quantity");
                        let p_entry = top_prods_map.entry((pk, name)).or_insert((0, 0));
                        p_entry.0 += qty;
                        p_entry.1 += amt;
                    }
                }
            }

            agg.by_method = by_method_map.into_iter().collect();
            agg.by_reason = by_reason_map
                .into_iter()
                .map(|(r, (c, a))| (r, c, a))
                .collect();
            let mut top_prods: Vec<_> = top_prods_map
                .into_iter()
                .map(|((pk, name), (q, a))| (pk, name, q, a))
                .collect();
            top_prods.sort_by_key(|b| std::cmp::Reverse(b.3));
            top_prods.truncate(10);
            agg.top_products = top_prods;

            Ok(agg)
        }
    }
}
