//! Copies the customer of each credit note's invoice onto credit notes saved without one, in a live
//! SQLite database or a MongoDB database holding any number of shops. It only fills blanks, and it
//! does not rewrite customers' stored totals.

use super::ticket_billing_migration::{in_shop, text};
use crate::core::error::AppResult;
use crate::core::tenancy::TENANT_FIELD;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use sqlx::{Executor, SqlitePool};

/// The credit note columns copied from the invoice, named alike on both.
const COPIED: [&str; 2] = ["customer_key", "customer_name_snapshot"];

/// What a MongoDB run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct CreditNoteCustomerReport {
    /// Credit notes missing something their invoice has.
    pub credit_notes: u64,
}

/// Fills an old SQLite database's credit notes from their invoices, once, in one transaction. Does
/// nothing when nothing is blank. The sync triggers are dropped first (`install_triggers` puts them
/// back) so none of this is uploaded: the cloud migrates itself with the same rule.
pub async fn migrate_credit_note_customers_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    const BLANK: &str = "i.key = credit_notes.invoice_key AND (\
        (credit_notes.customer_key IS NULL AND i.customer_key IS NOT NULL) OR \
        (credit_notes.customer_name_snapshot IS NULL AND i.customer_name_snapshot IS NOT NULL))";
    let blank: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM credit_notes JOIN invoices i ON {BLANK}"
    ))
    .fetch_one(pool)
    .await?;
    if blank == 0 {
        return Ok(());
    }
    tracing::info!("Copying the customer onto {blank} credit notes");

    let mut tx = pool.begin().await?;
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(&mut *tx)
    .await?;
    for trigger in triggers {
        tx.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }
    // `COALESCE` keeps what a note already says.
    tx.execute(
        format!(
            "UPDATE credit_notes SET \
             customer_key = COALESCE(credit_notes.customer_key, i.customer_key), \
             customer_name_snapshot = \
                 COALESCE(credit_notes.customer_name_snapshot, i.customer_name_snapshot) \
             FROM invoices i WHERE {BLANK}"
        )
        .as_str(),
    )
    .await?;
    tx.commit().await
}

/// Fills every shop's credit notes from their invoices on a MongoDB database. Idempotent. The
/// writes keep each note's version and time, so linked desktops do not take them as an edit; each
/// desktop fills its own copy when it starts.
pub async fn migrate_credit_note_customers_mongo(
    db: &Database,
    mode: Mode,
) -> AppResult<CreditNoteCustomerReport> {
    let notes = db.collection::<Document>("credit_notes");
    let invoices = db.collection::<Document>("invoices");
    let blanks: Vec<Document> = COPIED
        .iter()
        .map(|field| doc! { *field: Bson::Null })
        .collect();
    let mut cursor = notes
        .find(doc! { "invoice_key": { "$ne": Bson::Null }, "$or": blanks })
        .await?;

    let mut report = CreditNoteCustomerReport::default();
    while let Some(note) = cursor.try_next().await? {
        let shop = note.get_str(TENANT_FIELD).ok().map(str::to_string);
        let invoice_key = text(&note, "credit_notes", "invoice_key")?;
        let Some(invoice) = invoices
            .find_one(in_shop(&shop, doc! { "key": invoice_key }))
            .await?
        else {
            // A note whose invoice is not stored has nothing to copy.
            continue;
        };
        let mut filter = doc! { "key": text(&note, "credit_notes", "key")? };
        let mut set = Document::new();
        for field in COPIED {
            if matches!(note.get(field), None | Some(Bson::Null))
                && let Some(Bson::String(value)) = invoice.get(field)
            {
                filter.insert(field, Bson::Null);
                set.insert(field, value.as_str());
            }
        }
        if set.is_empty() {
            continue;
        }
        report.credit_notes += 1;
        if mode == Mode::Apply {
            notes
                .update_one(in_shop(&shop, filter), doc! { "$set": set })
                .await?;
        }
    }
    Ok(report)
}
