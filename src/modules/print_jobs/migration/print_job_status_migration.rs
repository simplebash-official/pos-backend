//! Applies the print job status rewrite to a live SQLite database or to a MongoDB database
//! holding any number of shops. Deliberately a plain status rewrite: nothing else on the job
//! changes, so its version and times stay as they were.

use crate::core::error::AppResult;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use sqlx::{Executor, SqlitePool};

/// The status the backend once accepted for a job that is being worked on.
pub const LEGACY_STATUS: &str = "in_progress";

/// The status a print job being worked on has now (the dashboard's "In production").
pub const CURRENT_STATUS: &str = "in_repair";

/// What a run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct Report {
    /// Print jobs that held the legacy status.
    pub print_jobs: u64,
}

/// Moves an old SQLite database's `in_progress` print jobs (soft-deleted ones included) to
/// `in_repair`, once. Does nothing when no job holds the legacy status. The sync triggers are
/// dropped first (`install_triggers` puts them back) so none of this is uploaded: the cloud
/// migrates itself with the same rule.
pub async fn migrate_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let stale: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM print_jobs WHERE status = ?")
        .bind(LEGACY_STATUS)
        .fetch_one(pool)
        .await?;
    if stale == 0 {
        return Ok(());
    }
    tracing::info!("Moving {stale} print jobs from {LEGACY_STATUS} to {CURRENT_STATUS}");
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(pool)
    .await?;
    for trigger in triggers {
        pool.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }
    sqlx::query("UPDATE print_jobs SET status = ? WHERE status = ?")
        .bind(CURRENT_STATUS)
        .bind(LEGACY_STATUS)
        .execute(pool)
        .await?;
    Ok(())
}

/// Moves every shop's `in_progress` print jobs on a MongoDB database to `in_repair`
/// (soft-deleted jobs included). Idempotent: a job that no longer holds the legacy status is
/// left alone. A dry run only counts.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> AppResult<Report> {
    let print_jobs = db.collection::<Document>("print_jobs");
    let stale = doc! { "status": LEGACY_STATUS };
    if mode != Mode::Apply {
        let found = print_jobs.count_documents(stale).await?;
        return Ok(Report { print_jobs: found });
    }
    let updated = print_jobs
        .update_many(stale, doc! { "$set": { "status": CURRENT_STATUS } })
        .await?;
    Ok(Report {
        print_jobs: updated.modified_count,
    })
}
