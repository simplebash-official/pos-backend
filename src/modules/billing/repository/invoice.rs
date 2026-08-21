// Mongo access for invoice records, specifically invoice mutation helpers
// for returns, cancellation, and updates.

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
/// and increments its `refunded_cents` total and version.
pub(crate) async fn update_invoice_return_progress(
    db: &Database,
    id: ObjectId,
    items: Vec<InvoiceItem>,
    additional_refunded_cents: i64,
) -> AppResult<Option<InvoiceDocument>> {
    let bson_items = items
        .iter()
        .map(|item| bson::serialize_to_document(item).map_err(crate::core::error::AppError::from))
        .collect::<Result<Vec<_>, _>>()?;
    let now = BsonDateTime::now();
    Ok(invoices(db)
        .find_one_and_update(
            doc! { "_id": id },
            doc! {
                "$set": {
                    "items": bson_items,
                    "updated_at": now,
                },
                "$inc": {
                    "refunded_cents": additional_refunded_cents,
                    "version": 1,
                }
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await?)
}
