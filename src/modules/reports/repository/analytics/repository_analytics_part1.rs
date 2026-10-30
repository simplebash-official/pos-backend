use super::super::sales;
use super::super::sales::get_i64_flexible;
use super::repository_analytics_part2::{AgingInvoiceRow, align_bucket_instant};
use super::repository_analytics_part3::{
    CashierAggRow, DiscountAgg, InvoiceKpiAggregate, date_match, invoices, trunc,
};
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
                    let revenue = get_i64_flexible(&row, "estimated_cost_cents");
                    let material = get_i64_flexible(&row, "material_cost_cents");
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
                    let revenue = get_i64_flexible(&doc, "estimated_cost_cents");
                    let material = get_i64_flexible(&doc, "material_cost_cents");
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
