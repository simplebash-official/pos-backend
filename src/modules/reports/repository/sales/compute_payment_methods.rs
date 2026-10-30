use super::get_i64_flexible::get_i64_flexible;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;
use crate::domain::reports::PaymentMethodBreakdown;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

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
