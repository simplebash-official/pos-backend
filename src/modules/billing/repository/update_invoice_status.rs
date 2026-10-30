use super::find_invoice_by_id::find_invoice_by_id;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, now_utc_iso};

use crate::core::error::AppResult;

use crate::modules::billing::model::InvoiceDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};

/// The one narrow mutator invoices ever get after creation — flips
/// `status` (and, for a void/close, the `voided_*`/`closed_*` fields) via
/// `$set`. No general "update an invoice" repository function exists (D3).
pub(crate) async fn update_invoice_status(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db)
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
            if let Ok(closed_at) = set_doc.get_datetime("closed_at") {
                assignments.push("closed_at = ?".to_string());
                bindings.push(bson_to_iso(closed_at));
            }
            if let Ok(closed_by) = set_doc.get_str("closed_by") {
                assignments.push("closed_by = ?".to_string());
                bindings.push(closed_by.to_string());
            }

            let sql = format!(
                "UPDATE invoices SET {} WHERE id = ?",
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

            find_invoice_by_id(db, id).await
        }
    }
}
