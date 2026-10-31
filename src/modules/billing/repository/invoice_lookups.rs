//! Invoice lookups and lists, including a session's invoices and payments.

use super::billing_shared::build_invoice_where_clause;
use super::billing_shared::invoice_from_sqlite_row;
use super::billing_shared::invoices;
use super::billing_shared::payment_from_sqlite_row;
use super::billing_shared::payments;
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;

use crate::core::error::AppResult;
use crate::modules::billing::model::{InvoiceDocument, PaymentDocument};
use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};
use sqlx::Row;

pub(crate) async fn find_invoice_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db).find_one(doc! { "_id": id }).await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query("SELECT * FROM invoices WHERE id = ? AND deleted_at IS NULL")
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}

pub(crate) async fn find_invoice_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<InvoiceDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_invoice_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    match db {
        Db::Mongo(db) => Ok(invoices(db)
            .find_one(doc! {
                "$or": [
                    { "key": id_or_key },
                    { "invoice_number": id_or_key },
                ]
            })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query(
                "SELECT * FROM invoices WHERE (key = ? OR invoice_number = ?) AND deleted_at IS NULL",
            )
            .bind(id_or_key)
            .bind(id_or_key)
            .fetch_optional(pool)
            .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}

pub(crate) async fn find_invoice_by_key(db: &Db, key: &str) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM invoices WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}

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

pub(crate) async fn list_invoices(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<InvoiceDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let total = invoices(db).count_documents(filter.clone()).await?;

            let mut cursor = invoices(db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let SqlFilter {
                clause: where_clause,
                bindings,
            } = build_invoice_where_clause(&filter);

            let count_sql = format!("SELECT COUNT(*) as total FROM invoices WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM invoices WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(invoice_from_sqlite_row(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}
