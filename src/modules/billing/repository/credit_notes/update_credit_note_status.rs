use super::credit_notes_collection::credit_notes_collection;
use super::find_credit_note_by_id::find_credit_note_by_id;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, now_utc_iso};

use crate::core::error::AppResult;
use crate::modules::billing::model::CreditNoteDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};

/// The one narrow mutator credit notes ever get after creation — flips
/// `status` (and, for a void, the `voided_*` fields) via `$set`. No general
/// "update a credit note" repository function exists (append-only, same
/// posture as invoices).
pub(crate) async fn update_credit_note_status(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<CreditNoteDocument>> {
    match db {
        Db::Mongo(db) => Ok(credit_notes_collection(db)
            .find_one_and_update(
                doc! { "_id": id },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_hex = id.to_hex();
            let mut assignments = vec![
                "version = version + 1".to_string(),
                "updated_at = ?".to_string(),
            ];
            let now_iso = now_utc_iso();
            let mut bindings: Vec<String> = vec![now_iso];

            if let Ok(status) = set_doc.get_str("status") {
                assignments.push("status = ?".to_string());
                bindings.push(status.to_string());
            }
            if let Ok(voided_at) = set_doc.get_datetime("voided_at") {
                assignments.push("voided_at = ?".to_string());
                bindings.push(bson_to_iso(voided_at));
            }
            if let Ok(voided_by) = set_doc.get_str("voided_by") {
                assignments.push("voided_by = ?".to_string());
                bindings.push(voided_by.to_string());
            }
            if let Ok(voided_reason) = set_doc.get_str("voided_reason") {
                assignments.push("voided_reason = ?".to_string());
                bindings.push(voided_reason.to_string());
            }

            let sql = format!(
                "UPDATE credit_notes SET {} WHERE id = ?",
                assignments.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for b in bindings {
                query = query.bind(b);
            }
            let res = query.bind(&id_hex).execute(pool).await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            find_credit_note_by_id(db, id).await
        }
    }
}
