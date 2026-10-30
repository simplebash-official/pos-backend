use super::find_by_id::find_by_id;

use super::quotations::quotations;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;

use crate::core::error::AppResult;

use crate::domain::quotations::{STATUS_ACTIVE, STATUS_CANCELLED};
use crate::modules::quotations::model::QuotationDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Marks an `active` quotation `cancelled`, bumping its sync `version` and
/// recording which device did it. Returns `None` when the row is missing *or*
/// already not active, so two racing cancels cannot both win;
/// `service::cancel_quotation` tells the two cases apart beforehand.
pub(crate) async fn cancel(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut set_doc = doc! {
                "status": STATUS_CANCELLED,
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
                "UPDATE quotations SET status = ?, updated_at = ?, updated_by_device = ?, \
                 version = version + 1 WHERE id = ? AND status = ?",
            )
            .bind(STATUS_CANCELLED)
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
