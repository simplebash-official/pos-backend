use super::job_columns::JOB_COLUMNS;
use super::job_err::job_err;
use super::jobs::JOBS;
use super::row_to_job::row_to_job;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::labels::model::LabelPrintJobDocument;

use mongodb::bson::doc;

/// One print job by its `lpj_` key.
pub(crate) async fn find_job_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<LabelPrintJobDocument>> {
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query(&format!(
                "SELECT {JOB_COLUMNS} FROM label_print_jobs WHERE key = ?1 LIMIT 1"
            ))
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|e| job_err("fetch", e))?;
            row.as_ref().map(row_to_job).transpose()
        }
        Db::Mongo(mongo) => Ok(mongo
            .collection::<LabelPrintJobDocument>(JOBS)
            .find_one(doc! { "key": key })
            .await?),
    }
}
