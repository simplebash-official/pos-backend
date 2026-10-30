use super::enum_text::enum_text;
use super::job_columns::JOB_COLUMNS;
use super::job_err::job_err;
use super::jobs::JOBS;
use super::labelled_entry::LabelledEntry;
use super::row_to_job::row_to_job;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::labels::LabelJobOutcome;
use crate::modules::labels::model::LabelPrintJobDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every sticker group of every job that was not reported as failed, newest
/// job first. Feeds the coverage view.
// shortcut: reads the whole history each time (fine for a shop's few thousand
// jobs a year); keep a per-product "last labelled" row if it ever grows large.
pub(crate) async fn list_labelled_entries(db: &Db) -> AppResult<Vec<LabelledEntry>> {
    let failed = enum_text(&LabelJobOutcome::Failed)?;
    let jobs: Vec<LabelPrintJobDocument> = match db {
        Db::Sqlite(pool) => {
            let rows = sqlx::query(&format!(
                "SELECT {JOB_COLUMNS} FROM label_print_jobs WHERE outcome != ?1 \
                 ORDER BY created_at DESC, key DESC"
            ))
            .bind(&failed)
            .fetch_all(pool)
            .await
            .map_err(|e| job_err("list", e))?;
            rows.iter().map(row_to_job).collect::<AppResult<_>>()?
        }
        Db::Mongo(mongo) => {
            mongo
                .collection::<LabelPrintJobDocument>(JOBS)
                .find(doc! { "outcome": { "$ne": failed } })
                .sort(doc! { "created_at": -1, "key": -1 })
                .await?
                .try_collect()
                .await?
        }
    };
    Ok(jobs
        .into_iter()
        .flat_map(|job| {
            let (key, created_at) = (job.key, job.created_at);
            job.items.into_iter().map(move |item| LabelledEntry {
                job_key: key.clone(),
                product_key: item.product_key,
                copies: item.copies,
                created_at,
            })
        })
        .collect())
}
