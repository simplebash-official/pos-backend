use super::print_jobs::print_jobs;
use super::row_to_print_job_doc::row_to_print_job_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::print_jobs::model::PrintJobDocument;

use mongodb::bson::doc;

pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => Ok(print_jobs(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM print_jobs WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| row_to_print_job_doc(&row)).transpose()
        }
    }
}
