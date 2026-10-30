use super::enum_text::enum_text;
use super::find_job_by_key::find_job_by_key;
use super::job_err::job_err;
use super::jobs::JOBS;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::domain::labels::LabelJobOutcome;
use crate::modules::labels::model::LabelPrintJobDocument;

use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Moves a job that is still `rendered` to `outcome`; `None` when no such job
/// is waiting (missing, or already reported).
pub(crate) async fn report_job_outcome(
    db: &Db,
    key: &str,
    outcome: LabelJobOutcome,
    error: Option<&str>,
) -> AppResult<Option<LabelPrintJobDocument>> {
    let now = BsonDateTime::now();
    let rendered = enum_text(&LabelJobOutcome::Rendered)?;
    let outcome_text = enum_text(&outcome)?;
    match db {
        Db::Sqlite(pool) => {
            let result = sqlx::query(
                "UPDATE label_print_jobs SET outcome = ?2, error = ?3, updated_at = ?4 \
                 WHERE key = ?1 AND outcome = ?5",
            )
            .bind(key)
            .bind(&outcome_text)
            .bind(error)
            .bind(bson_to_iso(&now))
            .bind(&rendered)
            .execute(pool)
            .await
            .map_err(|e| job_err("save", e))?;
            if result.rows_affected() == 0 {
                return Ok(None);
            }
            find_job_by_key(db, key).await
        }
        Db::Mongo(mongo) => Ok(mongo
            .collection::<LabelPrintJobDocument>(JOBS)
            .find_one_and_update(
                doc! { "key": key, "outcome": rendered },
                doc! { "$set": { "outcome": outcome_text, "error": error, "updated_at": now } },
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await?),
    }
}
