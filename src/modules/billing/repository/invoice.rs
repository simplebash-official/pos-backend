// Mongo and SQLite access for invoice records, specifically invoice mutation helpers
// for credit notes, voiding, and closing.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use mongodb::bson::{DateTime as BsonDateTime, doc, oid::ObjectId};

use crate::{
    clients::{db::Db, sqlite::now_utc_iso},
    core::error::AppResult,
    domain::billing::InvoiceItem,
    modules::billing::model::InvoiceDocument,
};

fn invoices(db: &TenantDatabase) -> ScopedCollection<InvoiceDocument> {
    db.collection("invoices")
}

/// Atomically updates an invoice's items (with updated `returned_quantity`)
/// and increments its `refunded_cents`/`credit_note_count` totals and
/// version. Used both when a credit note is created (positive
/// `refunded_cents_delta`/`credit_note_count_delta`) and when one is voided
/// (negative deltas reversing the same fields).
///
/// `expected_version` guards this as an optimistic-locking write: the filter
/// requires the document's current `version` to match, so a concurrent write
/// that already bumped it (e.g. a second credit note against the same
/// invoice) causes this to match zero documents and return `Ok(None)`
/// instead of silently overwriting the other write's `items`/totals. Callers
/// that need this write to actually succeed (credit-note creation) must
/// re-read the invoice and retry on `None`; callers where a lost update is
/// merely logged (credit-note void, a much lower-stakes reversal path) may
/// treat `None` as a best-effort miss.
pub(crate) async fn update_invoice_credit_note_progress(
    db: &Db,
    id: ObjectId,
    items: Vec<InvoiceItem>,
    refunded_cents_delta: i64,
    credit_note_count_delta: i64,
    expected_version: i64,
) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => {
            let bson_items = items
                .iter()
                .map(|item| {
                    bson::serialize_to_document(item).map_err(crate::core::error::AppError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let now = BsonDateTime::now();
            Ok(invoices(db)
                .find_one_and_update(
                    doc! { "_id": id, "version": expected_version },
                    doc! {
                        "$set": {
                            "items": bson_items,
                            "updated_at": now,
                        },
                        "$inc": {
                            "refunded_cents": refunded_cents_delta,
                            "credit_note_count": credit_note_count_delta,
                            "version": 1,
                        }
                    },
                )
                .return_document(mongodb::options::ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let mut tx = pool.begin().await?;
            let id_hex = id.to_hex();
            let row_opt = sqlx::query(
                "SELECT * FROM invoices WHERE id = ? AND version = ? AND deleted_at IS NULL",
            )
            .bind(&id_hex)
            .bind(expected_version)
            .fetch_optional(&mut *tx)
            .await?;

            let Some(_row) = row_opt else {
                return Ok(None);
            };

            let items_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
            let now_iso = now_utc_iso();

            sqlx::query(
                r#"
                UPDATE invoices SET
                    items = ?,
                    refunded_cents = refunded_cents + ?,
                    credit_note_count = credit_note_count + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE id = ? AND version = ?
                "#,
            )
            .bind(&items_json)
            .bind(refunded_cents_delta)
            .bind(credit_note_count_delta)
            .bind(&now_iso)
            .bind(&id_hex)
            .bind(expected_version)
            .execute(&mut *tx)
            .await?;

            let updated_row = sqlx::query("SELECT * FROM invoices WHERE id = ?")
                .bind(&id_hex)
                .fetch_one(&mut *tx)
                .await?;

            tx.commit().await?;

            let doc = super::invoice_from_sqlite_row(&updated_row)?;
            Ok(Some(doc))
        }
    }
}
