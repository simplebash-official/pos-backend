use super::print_jobs::print_jobs;
use super::row_to_print_job_doc::row_to_print_job_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::print_jobs::model::PrintJobDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_by_id(db: &Db, id: ObjectId) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => Ok(print_jobs(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM print_jobs WHERE id = ? AND deleted_at IS NULL")
                    .bind(id.to_hex())
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| row_to_print_job_doc(&row)).transpose()
        }
    }
}
