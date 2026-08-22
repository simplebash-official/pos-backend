// Mongo access for the `credit_notes` collection. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`/
// counts straight from the driver and leave the not-found -> AppError
// translation to `service`.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
};

use crate::{core::error::AppResult, modules::billing::model::CreditNoteDocument};

fn credit_notes(db: &Database) -> Collection<CreditNoteDocument> {
    db.collection("credit_notes")
}

/// Inserts a new credit note document and populates its auto-generated `_id`.
pub(crate) async fn insert_credit_note(
    db: &Database,
    mut document: CreditNoteDocument,
) -> AppResult<CreditNoteDocument> {
    let result = credit_notes(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// Look up a single credit note document by its Mongo ObjectId.
pub(crate) async fn find_credit_note_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<CreditNoteDocument>> {
    Ok(credit_notes(db).find_one(doc! { "_id": id }).await?)
}

/// Look up a single credit note document by its unique model key (`cn_...`).
pub(crate) async fn find_credit_note_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<CreditNoteDocument>> {
    Ok(credit_notes(db).find_one(doc! { "key": key }).await?)
}

/// Look up a single credit note document by either its hex ObjectId or unique model key.
pub(crate) async fn find_credit_note_by_id_or_key(
    db: &Database,
    id_or_key: &str,
) -> AppResult<Option<CreditNoteDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_credit_note_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_credit_note_by_key(db, id_or_key).await
}

/// Lists credit note documents matching a filter, sorted newest first with pagination.
pub(crate) async fn list_credit_notes(
    db: &Database,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<CreditNoteDocument>, u64)> {
    let total = count_credit_notes(db, filter.clone()).await?;

    let mut cursor = credit_notes(db)
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

/// Returns the count of credit note documents matching the specified filter.
pub(crate) async fn count_credit_notes(db: &Database, filter: Document) -> AppResult<u64> {
    Ok(credit_notes(db).count_documents(filter).await?)
}

/// Count of non-voided credit notes against an invoice — the guard behind
/// `service::sale::close_invoice`'s "no open credit note" rule.
pub(crate) async fn count_open_credit_notes_for_invoice(
    db: &Database,
    invoice_key: &str,
) -> AppResult<u64> {
    count_credit_notes(
        db,
        doc! { "invoice_key": invoice_key, "status": { "$ne": "voided" } },
    )
    .await
}

/// The one narrow mutator credit notes ever get after creation — flips
/// `status` (and, for a void, the `voided_*` fields) via `$set`. No general
/// "update a credit note" repository function exists (append-only, same
/// posture as invoices).
pub(crate) async fn update_credit_note_status(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<CreditNoteDocument>> {
    Ok(credit_notes(db)
        .find_one_and_update(
            doc! { "_id": id },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await?)
}
