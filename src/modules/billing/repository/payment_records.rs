//! Payment storage and per-invoice payment queries.

use super::billing_shared::invoices;
use super::billing_shared::payment_from_sqlite_row;
use super::billing_shared::payments;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::billing::model::PaymentDocument;
use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use sqlx::Row;

/// Used by `service::sale::cancel_invoice`'s guard (D8): cancellation is
/// blocked once more than the original sale-time payment(s) exist.
pub(crate) async fn count_payments_for_invoice(db: &Db, invoice_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(payments(db)
            .count_documents(doc! { "invoice_key": invoice_key })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT COUNT(*) as cnt FROM payments WHERE invoice_key = ? AND deleted_at IS NULL",
            )
            .bind(invoice_key)
            .fetch_one(pool)
            .await?;
            let cnt: i64 = row.get("cnt");
            Ok(cnt as u64)
        }
    }
}

/// Looks up a single payment by its unique model key (`pay_...`) — used by
/// `service::credit_notes::void_credit_note` to read back the original
/// amount/method of a refund leg it's about to reverse.
pub(crate) async fn find_payment_by_key(db: &Db, key: &str) -> AppResult<Option<PaymentDocument>> {
    match db {
        Db::Mongo(db) => Ok(payments(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM payments WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| payment_from_sqlite_row(&row)).transpose()
        }
    }
}

pub(crate) async fn insert_payment(
    db: &Db,
    mut document: PaymentDocument,
) -> AppResult<PaymentDocument> {
    match db {
        Db::Mongo(db) => {
            let result = payments(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let recorded_at_iso = bson_to_iso(&document.recorded_at);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);

            sqlx::query(
                r#"
                INSERT INTO payments (
                    key, id, invoice_key, session_key, amount_cents, payment_method, notes,
                    recorded_by_user_id, recorded_by_name_snapshot, recorded_at,
                    version, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.invoice_key)
            .bind(&document.session_key)
            .bind(document.amount_cents)
            .bind(&document.payment_method)
            .bind(&document.notes)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
            .bind(&recorded_at_iso)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

pub(crate) async fn list_payments_for_invoice(
    db: &Db,
    invoice_key: &str,
) -> AppResult<Vec<PaymentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payments(db)
                .find(doc! { "invoice_key": invoice_key })
                .sort(doc! { "recorded_at": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM payments WHERE invoice_key = ? AND deleted_at IS NULL ORDER BY recorded_at ASC",
            )
            .bind(invoice_key)
            .fetch_all(pool)
            .await?;

            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(payment_from_sqlite_row(&row)?);
            }
            Ok(items)
        }
    }
}

/// What customers paid, by method, from `since` (`YYYY-MM-DD`, UTC) on, as
/// `(method, received, refunded)`. A refund is a negative payment row; payments on voided invoices
/// and deleted payments are left out, like the shift totals.
///
/// shortcut: the Mongo side sums in memory over the payments of the range; a shop's payments fit,
/// upgrade to a `$group` pipeline when a shop passes hundreds of thousands.
pub(crate) async fn payment_totals_by_method(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(String, i64, i64)>> {
    let mut totals: std::collections::BTreeMap<String, (i64, i64)> =
        std::collections::BTreeMap::new();
    let mut add = |method: String, amount: i64| {
        let entry = totals.entry(method).or_default();
        if amount >= 0 {
            entry.0 += amount;
        } else {
            entry.1 += -amount;
        }
    };
    match db {
        Db::Mongo(db) => {
            let from = chrono::NaiveDate::parse_from_str(since, "%Y-%m-%d")
                .map_err(|_| crate::core::error::AppError::validation("Invalid date."))?
                .and_hms_opt(0, 0, 0)
                .expect("midnight exists")
                .and_utc();
            let voided: std::collections::HashSet<String> = {
                let mut cursor = invoices(db)
                    .find(
                        doc! { "$or": [{ "status": "voided" }, { "voided_at": { "$ne": null } }] },
                    )
                    .await?;
                let mut keys = std::collections::HashSet::new();
                while let Some(invoice) = cursor.try_next().await? {
                    keys.insert(invoice.key);
                }
                keys
            };
            let mut cursor = payments(db)
                .find(doc! { "deleted_at": null, "recorded_at": { "$gte": BsonDateTime::from_chrono(from) } })
                .await?;
            while let Some(p) = cursor.try_next().await? {
                if !voided.contains(&p.invoice_key) {
                    add(p.payment_method, p.amount_cents);
                }
            }
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT payment_method, amount_cents FROM payments
                 WHERE deleted_at IS NULL AND recorded_at >= ?
                   AND invoice_key NOT IN (SELECT key FROM invoices WHERE status = 'voided' OR voided_at IS NOT NULL)",
            )
            .bind(since)
            .fetch_all(pool)
            .await?;
            for row in rows {
                add(row.get("payment_method"), row.get("amount_cents"));
            }
        }
    }
    Ok(totals
        .into_iter()
        .map(|(method, (received, refunded))| (method, received, refunded))
        .collect())
}
