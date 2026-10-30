use super::print_jobs::print_jobs;
use super::row_to_print_job_doc::row_to_print_job_doc;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;

use crate::core::error::AppResult;
use crate::modules::print_jobs::model::PrintJobDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn delete(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let mut set_doc = doc! { "deleted_at": now, "updated_at": now };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(print_jobs(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": { "$exists": false } },
                    doc! { "$set": set_doc, "$inc": { "version": 1 } },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let now_iso = now_utc_iso();
            let id_hex = id.to_hex();
            let res = sqlx::query(
                "UPDATE print_jobs SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(device_id)
            .bind(&id_hex)
            .execute(pool)
            .await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            let row_opt = sqlx::query("SELECT * FROM print_jobs WHERE id = ?")
                .bind(&id_hex)
                .fetch_optional(pool)
                .await?;

            row_opt.map(|row| row_to_print_job_doc(&row)).transpose()
        }
    }
}
