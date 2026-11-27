//! Applies the billing marks decided by `plan` to a live SQLite database or to a MongoDB database
//! holding any number of shops.

use super::ticket_billing::{InvoiceFacts, InvoiceLine, Stamp, TicketKind, plan};
use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::TENANT_FIELD;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use sqlx::{Executor, Row, SqlitePool};
use std::collections::{BTreeMap, HashSet};

/// What a run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct Report {
    /// Tickets an invoice billed that carried no billing mark.
    pub tickets: u64,
}

fn protocol(err: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Protocol(err.to_string())
}

fn table_of(kind: TicketKind) -> &'static str {
    match kind {
        TicketKind::Repair => "repairs",
        TicketKind::PrintJob => "print_jobs",
    }
}

fn facts_of(
    key: String,
    number: String,
    created_at: DateTime<Utc>,
    is_voided: bool,
    lines: &[InvoiceLine],
) -> InvoiceFacts {
    InvoiceFacts {
        key,
        number,
        created_at,
        is_voided,
        tickets: lines.iter().filter_map(InvoiceLine::ticket).collect(),
    }
}

/// Marks an old SQLite database's repair jobs and print jobs with the invoice that billed them,
/// once. Does nothing when every billed ticket is already marked. The sync triggers are dropped
/// first (`install_triggers` puts them back) so none of this is uploaded: the cloud migrates
/// itself with the same rule.
pub async fn migrate_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut marked = HashSet::new();
    for kind in [TicketKind::Repair, TicketKind::PrintJob] {
        let keys: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT key FROM {} WHERE billed_invoice_key IS NOT NULL",
            table_of(kind)
        ))
        .fetch_all(pool)
        .await?;
        marked.extend(keys.into_iter().map(|key| (kind, key)));
    }

    let mut invoices = Vec::new();
    for row in sqlx::query(
        "SELECT key, invoice_number, status, created_at, items FROM invoices \
         WHERE deleted_at IS NULL AND items LIKE '%sourceTicketKey%'",
    )
    .fetch_all(pool)
    .await?
    {
        let lines: Vec<InvoiceLine> =
            serde_json::from_str(&row.get::<String, _>("items")).map_err(protocol)?;
        let created_at = DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
            .map_err(protocol)?
            .with_timezone(&Utc);
        invoices.push(facts_of(
            row.get("key"),
            row.get("invoice_number"),
            created_at,
            row.get::<String, _>("status") == "voided",
            &lines,
        ));
    }

    let stamps = plan(&invoices, &marked);
    if stamps.is_empty() {
        return Ok(());
    }
    tracing::info!(
        "Marking {} repair and print jobs with the invoice that billed them",
        stamps.len()
    );
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(pool)
    .await?;
    for trigger in triggers {
        pool.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }
    for stamp in &stamps {
        sqlx::query(&format!(
            "UPDATE {} SET billed_invoice_key = ?, billed_invoice_number = ? \
             WHERE key = ? AND billed_invoice_key IS NULL",
            table_of(stamp.kind)
        ))
        .bind(&stamp.invoice_key)
        .bind(&stamp.invoice_number)
        .bind(&stamp.ticket_key)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub(super) fn text<'a>(
    document: &'a Document,
    collection: &str,
    field: &str,
) -> AppResult<&'a str> {
    document
        .get_str(field)
        .map_err(|e| AppError::internal(format!("{collection}.{field}: {e}")))
}

/// Narrows a filter to one shop, or to the documents that carry no shop at all (a single-shop
/// database).
pub(super) fn in_shop(shop: &Option<String>, mut filter: Document) -> Document {
    match shop {
        Some(shop) => filter.insert(TENANT_FIELD, shop.as_str()),
        None => filter.insert(TENANT_FIELD, Bson::Null),
    };
    filter
}

/// What one shop's rows say: its invoices, and the tickets that already carry a mark.
#[derive(Debug, Default)]
struct ShopRows {
    invoices: Vec<InvoiceFacts>,
    marked: HashSet<(TicketKind, String)>,
}

/// Marks every shop's repair jobs and print jobs on a MongoDB database with the invoice that
/// billed them (soft-deleted tickets included). Idempotent: a ticket that carries a mark is left
/// alone.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> AppResult<Report> {
    let mut shops: BTreeMap<Option<String>, ShopRows> = BTreeMap::new();
    let mut cursor = db
        .collection::<Document>("invoices")
        .find(doc! {
            "deleted_at": Bson::Null,
            "items.sourceTicketKey": { "$exists": true },
        })
        .await?;
    while let Some(invoice) = cursor.try_next().await? {
        let lines: Vec<InvoiceLine> = mongodb::bson::deserialize_from_bson(
            invoice
                .get("items")
                .cloned()
                .ok_or_else(|| AppError::internal("invoices.items: missing"))?,
        )
        .map_err(|e| AppError::internal(format!("invoices.items: {e}")))?;
        let created_at = invoice
            .get_datetime("created_at")
            .map_err(|e| AppError::internal(format!("invoices.created_at: {e}")))?
            .to_chrono();
        let shop = invoice.get_str(TENANT_FIELD).ok().map(str::to_string);
        shops.entry(shop).or_default().invoices.push(facts_of(
            text(&invoice, "invoices", "key")?.to_string(),
            text(&invoice, "invoices", "invoice_number")?.to_string(),
            created_at,
            text(&invoice, "invoices", "status")? == "voided",
            &lines,
        ));
    }
    for kind in [TicketKind::Repair, TicketKind::PrintJob] {
        let name = table_of(kind);
        let mut cursor = db
            .collection::<Document>(name)
            .find(doc! { "billed_invoice_key": { "$ne": Bson::Null } })
            .await?;
        while let Some(ticket) = cursor.try_next().await? {
            let shop = ticket.get_str(TENANT_FIELD).ok().map(str::to_string);
            if let Some(entry) = shops.get_mut(&shop) {
                entry
                    .marked
                    .insert((kind, text(&ticket, name, "key")?.to_string()));
            }
        }
    }

    let mut report = Report::default();
    for (shop, rows) in shops {
        let stamps: Vec<Stamp> = plan(&rows.invoices, &rows.marked);
        report.tickets += stamps.len() as u64;
        if mode != Mode::Apply {
            continue;
        }
        for stamp in &stamps {
            db.collection::<Document>(table_of(stamp.kind))
                .update_one(
                    in_shop(
                        &shop,
                        doc! { "key": &stamp.ticket_key, "billed_invoice_key": Bson::Null },
                    ),
                    doc! { "$set": {
                        "billed_invoice_key": &stamp.invoice_key,
                        "billed_invoice_number": &stamp.invoice_number,
                    } },
                )
                .await?;
        }
    }
    Ok(report)
}
