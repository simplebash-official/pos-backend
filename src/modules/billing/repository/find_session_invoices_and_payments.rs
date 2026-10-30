use super::invoice_from_sqlite_row::invoice_from_sqlite_row;
use super::invoices::invoices;
use super::payment_from_sqlite_row::payment_from_sqlite_row;
use super::payments::payments;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::model::{InvoiceDocument, PaymentDocument};

use futures_util::TryStreamExt;

use mongodb::bson::doc;

/// Returns all non-voided invoices and valid payment transactions recorded for a
/// specific register session key across SQLite and MongoDB.
pub(crate) async fn find_session_invoices_and_payments(
    db: &Db,
    session_key: &str,
) -> AppResult<(Vec<InvoiceDocument>, Vec<PaymentDocument>)> {
    match db {
        Db::Mongo(db) => {
            let mut invoice_cursor = invoices(db)
                .find(doc! {
                    "session_key": session_key,
                    "status": { "$ne": "voided" },
                    "voided_at": null,
                    "deleted_at": null,
                })
                .sort(doc! { "created_at": 1 })
                .await?;

            let mut session_invoices = Vec::new();
            while let Some(document) = invoice_cursor.try_next().await? {
                session_invoices.push(document);
            }

            let mut payment_cursor = payments(db)
                .find(doc! {
                    "session_key": session_key,
                    "deleted_at": null,
                })
                .sort(doc! { "recorded_at": 1 })
                .await?;

            let mut session_payments = Vec::new();
            while let Some(document) = payment_cursor.try_next().await? {
                session_payments.push(document);
            }

            let voided_invoice_keys: std::collections::HashSet<String> = {
                let mut voided_cursor = invoices(db)
                    .find(doc! {
                        "session_key": session_key,
                        "$or": [
                            { "status": "voided" },
                            { "voided_at": { "$ne": null } }
                        ]
                    })
                    .await?;
                let mut keys = std::collections::HashSet::new();
                while let Some(voided) = voided_cursor.try_next().await? {
                    keys.insert(voided.key);
                }
                keys
            };

            session_payments.retain(|p| !voided_invoice_keys.contains(&p.invoice_key));

            Ok((session_invoices, session_payments))
        }
        Db::Sqlite(pool) => {
            let invoice_rows = sqlx::query(
                "SELECT * FROM invoices WHERE session_key = ? AND status != 'voided' AND voided_at IS NULL AND deleted_at IS NULL ORDER BY created_at ASC",
            )
            .bind(session_key)
            .fetch_all(pool)
            .await?;

            let mut invoices = Vec::with_capacity(invoice_rows.len());
            for row in invoice_rows {
                invoices.push(invoice_from_sqlite_row(&row)?);
            }

            let payment_rows = sqlx::query(
                "SELECT * FROM payments WHERE session_key = ? AND deleted_at IS NULL AND invoice_key NOT IN (SELECT key FROM invoices WHERE status = 'voided' OR voided_at IS NOT NULL) ORDER BY recorded_at ASC",
            )
            .bind(session_key)
            .fetch_all(pool)
            .await?;

            let mut payments = Vec::with_capacity(payment_rows.len());
            for row in payment_rows {
                payments.push(payment_from_sqlite_row(&row)?);
            }

            Ok((invoices, payments))
        }
    }
}
