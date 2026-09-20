// MongoDB aggregation queries for sales and revenue analysis over the `invoices` collection.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

use crate::{
    clients::{db::Db, sqlite::map_sqlite_row_to_document},
    core::error::AppResult,
    domain::reports::{DailySalesReportSummary, PaymentMethodBreakdown, TopProductEntry},
};

fn invoices(db: &TenantDatabase) -> ScopedCollection<Document> {
    db.collection("invoices")
}

pub(crate) fn get_i64_flexible(doc: &Document, key: &str) -> i64 {
    doc.get_i64(key)
        .ok()
        .or_else(|| doc.get_i32(key).ok().map(i64::from))
        .or_else(|| doc.get_f64(key).ok().map(|v| v as i64))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SalesSummaryAggregate {
    pub total_invoices: u64,
    pub gross_sales_cents: i64,
    pub discount_cents: i64,
    pub total_sales_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub payment_methods: PaymentMethodBreakdown,
}

/// Runs a `$facet` aggregation over `invoices` within `[start, end)` to compute
/// overall totals, stream breakdowns (retail, repair, print), and payment methods.
pub(crate) async fn aggregate_sales_summary(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<SalesSummaryAggregate> {
    match db {
        Db::Mongo(db) => {
            let match_stage = doc! {
                "$match": {
                    "status": { "$ne": "voided" },
                    "created_at": {
                        "$gte": BsonDateTime::from_chrono(start),
                        "$lt": BsonDateTime::from_chrono(end),
                    }
                }
            };

            let pipeline = vec![
                match_stage.clone(),
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "subtotal": { "$sum": "$subtotal_cents" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "total": { "$sum": "$total_cents" },
                                }
                            }
                        ],
                        "streams": [
                            { "$unwind": "$items" },
                            {
                                "$group": {
                                    "_id": { "$ifNull": ["$items.sourceType", "$items.source_type"] },
                                    "revenue": { "$sum": { "$ifNull": ["$items.totalCents", "$items.total_cents"] } },
                                }
                            }
                        ],
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut agg = SalesSummaryAggregate::default();

            if let Some(result) = cursor.try_next().await? {
                if let Ok(totals) = result.get_array("totals")
                    && let Some(first) = totals.first().and_then(|v| v.as_document())
                {
                    agg.total_invoices = get_i64_flexible(first, "count") as u64;
                    agg.gross_sales_cents = get_i64_flexible(first, "subtotal");
                    agg.discount_cents = get_i64_flexible(first, "discount");
                    agg.total_sales_cents = get_i64_flexible(first, "total");
                }

                if let Ok(streams) = result.get_array("streams") {
                    for stream in streams {
                        if let Some(doc) = stream.as_document() {
                            let source_type = doc.get_str("_id").unwrap_or("");
                            let revenue = get_i64_flexible(doc, "revenue");
                            match source_type {
                                "retail" => agg.retail_revenue_cents += revenue,
                                "repair" => agg.repair_revenue_cents += revenue,
                                "print" => agg.print_revenue_cents += revenue,
                                _ => {}
                            }
                        }
                    }
                }
            }

            agg.payment_methods =
                compute_payment_methods(&Db::Mongo(db.clone()), start, end).await?;
            Ok(agg)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT * FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = SalesSummaryAggregate::default();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                agg.total_invoices += 1;
                agg.gross_sales_cents += get_i64_flexible(&doc, "subtotal_cents");
                agg.discount_cents += get_i64_flexible(&doc, "discount_cents");
                agg.total_sales_cents += get_i64_flexible(&doc, "total_cents");

                if let Ok(items) = doc.get_array("items") {
                    for item in items {
                        if let Some(idoc) = item.as_document() {
                            let source_type = idoc
                                .get_str("sourceType")
                                .or_else(|_| idoc.get_str("source_type"))
                                .unwrap_or("");
                            let revenue = if idoc.contains_key("totalCents") {
                                get_i64_flexible(idoc, "totalCents")
                            } else {
                                get_i64_flexible(idoc, "total_cents")
                            };
                            match source_type {
                                "retail" => agg.retail_revenue_cents += revenue,
                                "repair" => agg.repair_revenue_cents += revenue,
                                "print" => agg.print_revenue_cents += revenue,
                                _ => {}
                            }
                        }
                    }
                }
            }

            agg.payment_methods = compute_payment_methods(db, start, end).await?;
            Ok(agg)
        }
    }
}

fn accumulate_payment_doc(doc: &Document, breakdown: &mut PaymentMethodBreakdown) {
    let payment_method = doc.get_str("payment_method").unwrap_or("cash");
    let is_credit = doc.get_bool("is_credit").unwrap_or(false);
    let total_cents = get_i64_flexible(doc, "total_cents");
    let amount_received_cents = get_i64_flexible(doc, "amount_received_cents");

    if is_credit {
        let upfront = amount_received_cents;
        let balance = (total_cents - upfront).max(0);
        breakdown.credit_cents += balance;
        if upfront > 0 {
            match payment_method {
                "card" => breakdown.card_cents += upfront,
                "online" => breakdown.online_cents += upfront,
                _ => breakdown.cash_cents += upfront,
            }
        }
    } else if payment_method == "split" {
        if let Ok(splits) = doc.get_array("split_payments") {
            for split in splits {
                if let Some(sdoc) = split.as_document() {
                    let method = sdoc.get_str("method").unwrap_or("cash");
                    let amt = get_i64_flexible(sdoc, "amount_cents");
                    match method {
                        "card" => breakdown.card_cents += amt,
                        "online" => breakdown.online_cents += amt,
                        _ => breakdown.cash_cents += amt,
                    }
                }
            }
        }
    } else {
        match payment_method {
            "card" => breakdown.card_cents += total_cents,
            "online" => breakdown.online_cents += total_cents,
            "credit" => breakdown.credit_cents += total_cents,
            _ => breakdown.cash_cents += total_cents,
        }
    }
}

/// Helper to accurately compute payment breakdown across cash, card, online, credit, and split payments.
/// Reused by `repository::analytics` for the range-scoped payment-methods endpoint.
pub(crate) async fn compute_payment_methods(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<PaymentMethodBreakdown> {
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
                    "$project": {
                        "payment_method": 1,
                        "is_credit": 1,
                        "total_cents": 1,
                        "amount_received_cents": 1,
                        "split_payments": 1,
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut breakdown = PaymentMethodBreakdown::default();

            while let Some(doc) = cursor.try_next().await? {
                accumulate_payment_doc(&doc, &mut breakdown);
            }

            Ok(breakdown)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT payment_method, is_credit, total_cents, amount_received_cents, split_payments FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut breakdown = PaymentMethodBreakdown::default();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                accumulate_payment_doc(&doc, &mut breakdown);
            }

            Ok(breakdown)
        }
    }
}

fn accumulate_daily_sales_doc(
    doc: &Document,
    date: String,
    map: &mut std::collections::BTreeMap<String, DailySalesReportSummary>,
) {
    let subtotal = get_i64_flexible(doc, "subtotal_cents");
    let discount = get_i64_flexible(doc, "discount_cents");
    let total = get_i64_flexible(doc, "total_cents");

    let entry = map
        .entry(date.clone())
        .or_insert_with(|| DailySalesReportSummary {
            date,
            gross_sales_cents: 0,
            discount_cents: 0,
            total_sales_cents: 0,
            total_invoices: 0,
            repair_revenue_cents: 0,
            print_revenue_cents: 0,
            retail_revenue_cents: 0,
            payment_methods: PaymentMethodBreakdown::default(),
        });

    entry.gross_sales_cents += subtotal;
    entry.discount_cents += discount;
    entry.total_sales_cents += total;
    entry.total_invoices += 1;

    if let Ok(items) = doc.get_array("items") {
        for item in items {
            if let Some(idoc) = item.as_document() {
                let source_type = idoc
                    .get_str("sourceType")
                    .or_else(|_| idoc.get_str("source_type"))
                    .unwrap_or("");
                let item_total = if idoc.contains_key("totalCents") {
                    get_i64_flexible(idoc, "totalCents")
                } else {
                    get_i64_flexible(idoc, "total_cents")
                };
                match source_type {
                    "retail" => entry.retail_revenue_cents += item_total,
                    "repair" => entry.repair_revenue_cents += item_total,
                    "print" => entry.print_revenue_cents += item_total,
                    _ => {}
                }
            }
        }
    }

    let payment_method = doc.get_str("payment_method").unwrap_or("cash");
    let is_credit = doc.get_bool("is_credit").unwrap_or(false);
    let amount_received = get_i64_flexible(doc, "amount_received_cents");

    if is_credit {
        let balance = (total - amount_received).max(0);
        entry.payment_methods.credit_cents += balance;
        if amount_received > 0 {
            match payment_method {
                "card" => entry.payment_methods.card_cents += amount_received,
                "online" => entry.payment_methods.online_cents += amount_received,
                _ => entry.payment_methods.cash_cents += amount_received,
            }
        }
    } else if payment_method == "split" {
        if let Ok(splits) = doc.get_array("split_payments") {
            for split in splits {
                if let Some(sdoc) = split.as_document() {
                    let method = sdoc.get_str("method").unwrap_or("cash");
                    let amt = get_i64_flexible(sdoc, "amount_cents");
                    match method {
                        "card" => entry.payment_methods.card_cents += amt,
                        "online" => entry.payment_methods.online_cents += amt,
                        _ => entry.payment_methods.cash_cents += amt,
                    }
                }
            }
        }
    } else {
        match payment_method {
            "card" => entry.payment_methods.card_cents += total,
            "online" => entry.payment_methods.online_cents += total,
            "credit" => entry.payment_methods.credit_cents += total,
            _ => entry.payment_methods.cash_cents += total,
        }
    }
}

/// Aggregates sales grouped by calendar day (UTC) in `[start, end)`.
pub(crate) async fn aggregate_daily_sales(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<DailySalesReportSummary>> {
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
                    "$project": {
                        "date": {
                            "$dateToString": {
                                "format": "%Y-%m-%d",
                                "date": "$created_at",
                            }
                        },
                        "subtotal_cents": 1,
                        "discount_cents": 1,
                        "total_cents": 1,
                        "items": 1,
                        "payment_method": 1,
                        "is_credit": 1,
                        "amount_received_cents": 1,
                        "split_payments": 1,
                    }
                },
                doc! {
                    "$sort": { "created_at": 1 }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut map: std::collections::BTreeMap<String, DailySalesReportSummary> =
                std::collections::BTreeMap::new();

            while let Some(doc) = cursor.try_next().await? {
                let date = doc.get_str("date").unwrap_or("").to_string();
                accumulate_daily_sales_doc(&doc, date, &mut map);
            }

            Ok(map.into_values().collect())
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT * FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ? ORDER BY created_at ASC"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::BTreeMap<String, DailySalesReportSummary> =
                std::collections::BTreeMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let date = if let Ok(bdt) = doc.get_datetime("created_at") {
                    bdt.to_chrono().format("%Y-%m-%d").to_string()
                } else if let Ok(s) = doc.get_str("created_at") {
                    s.chars().take(10).collect()
                } else {
                    String::new()
                };
                accumulate_daily_sales_doc(&doc, date, &mut map);
            }

            Ok(map.into_values().collect())
        }
    }
}

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

            #[derive(Default)]
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
