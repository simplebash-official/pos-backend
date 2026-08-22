// Mongo access for invoice records, specifically invoice mutation helpers
// for credit notes, voiding, and closing.

use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, doc, oid::ObjectId},
};

use crate::{
    core::error::AppResult, domain::billing::InvoiceItem, modules::billing::model::InvoiceDocument,
};

fn invoices(db: &Database) -> Collection<InvoiceDocument> {
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
    db: &Database,
    id: ObjectId,
    items: Vec<InvoiceItem>,
    refunded_cents_delta: i64,
    credit_note_count_delta: i64,
    expected_version: i64,
) -> AppResult<Option<InvoiceDocument>> {
    let bson_items = items
        .iter()
        .map(|item| bson::serialize_to_document(item).map_err(crate::core::error::AppError::from))
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
