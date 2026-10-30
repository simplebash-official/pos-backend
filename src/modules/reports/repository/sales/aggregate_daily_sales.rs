use super::get_i64_flexible::get_i64_flexible;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;
use crate::domain::reports::{DailySalesReportSummary, PaymentMethodBreakdown};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

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
