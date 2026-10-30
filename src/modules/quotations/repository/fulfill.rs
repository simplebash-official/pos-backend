use super::find_by_id::find_by_id;

use super::quotations::quotations;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;

use crate::core::error::AppResult;

use crate::domain::quotations::{STATUS_ACTIVE, STATUS_FULFILLED};
use crate::modules::quotations::model::QuotationDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Fulfills an active quotation, transitioning status to `fulfilled` and linking
/// to the invoice that converted it.
pub(crate) async fn fulfill(
    db: &Db,
    id: ObjectId,
    invoice_key: &str,
    device_id: Option<String>,
) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut set_doc = doc! {
                "status": STATUS_FULFILLED,
                "fulfilled_by_invoice_key": invoice_key,
                "fulfilled_at": BsonDateTime::now(),
                "updated_at": BsonDateTime::now(),
            };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(quotations(db)
                .find_one_and_update(
                    doc! { "_id": id, "status": STATUS_ACTIVE },
                    doc! { "$set": set_doc, "$inc": { "version": 1 } },
                )
                .return_document(mongodb::options::ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let res = sqlx::query(
                "UPDATE quotations SET status = ?, fulfilled_by_invoice_key = ?, fulfilled_at = ?, \
                 updated_at = ?, updated_by_device = ?, version = version + 1 \
                 WHERE id = ? AND status = ?",
            )
            .bind(STATUS_FULFILLED)
            .bind(invoice_key)
            .bind(now_utc_iso())
            .bind(now_utc_iso())
            .bind(&device_id)
            .bind(id.to_hex())
            .bind(STATUS_ACTIVE)
            .execute(pool)
            .await?;
            if res.rows_affected() == 0 {
                return Ok(None);
            }
            find_by_id(db, id).await
        }
    }
}
