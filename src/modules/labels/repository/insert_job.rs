use super::enum_text::enum_text;
use super::job_columns::JOB_COLUMNS;
use super::job_err::job_err;
use super::jobs::JOBS;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;

use crate::modules::labels::model::LabelPrintJobDocument;

/// Records a new print job.
pub(crate) async fn insert_job(db: &Db, job: &LabelPrintJobDocument) -> AppResult<()> {
    match db {
        Db::Sqlite(pool) => {
            let items = serde_json::to_string(&job.items).map_err(|e| job_err("save", e))?;
            sqlx::query(&format!(
                "INSERT INTO label_print_jobs ({JOB_COLUMNS}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)"
            ))
            .bind(&job.key)
            .bind(&job.template_key)
            .bind(&job.template_name)
            .bind(job.width_mm)
            .bind(job.height_mm)
            .bind(enum_text(&job.action)?)
            .bind(job.method.as_ref().map(enum_text).transpose()?)
            .bind(&job.printer_name)
            .bind(job.dpi.map(i64::from))
            .bind(enum_text(&job.outcome)?)
            .bind(&job.error)
            .bind(&job.printed_by_user_id)
            .bind(&job.printed_by_name)
            .bind(&items)
            .bind(i64::from(job.total_labels))
            .bind(bson_to_iso(&job.created_at))
            .bind(bson_to_iso(&job.updated_at))
            .execute(pool)
            .await
            .map_err(|e| job_err("save", e))?;
            Ok(())
        }
        Db::Mongo(mongo) => {
            mongo
                .collection::<LabelPrintJobDocument>(JOBS)
                .insert_one(job)
                .await?;
            Ok(())
        }
    }
}
