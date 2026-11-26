//! Moves old print jobs to lines (`legacy_items`) on a live SQLite database or on a MongoDB
//! database holding any number of shops. Nothing else on a job changes, so its version and times
//! stay as they were.

use super::legacy_print_job::{LegacyPrintJob, legacy_items};
use crate::core::error::{AppError, AppResult};
use crate::domain::print_jobs::PrintJobKind;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use sqlx::{Executor, Row, SqlitePool};

/// What a run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct ItemsReport {
    /// Jobs given their lines.
    pub print_jobs: u64,
    /// Of those, jobs that had a price or a material cost and so got a line.
    pub with_items: u64,
}

fn protocol(err: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Protocol(err.to_string())
}

fn kind_json() -> String {
    serde_json::to_string(&PrintJobKind::Standard).expect("a print job kind serializes to JSON")
}

/// Moves an old SQLite database's print jobs to lines, once, in one transaction: each job's
/// quote and material cost become its lines, and the old columns are dropped. Does nothing once
/// `print_jobs.estimated_cost_cents` is gone. The sync triggers name columns, so they are dropped
/// first (`install_triggers` puts them back) and none of this is uploaded: the cloud migrates
/// itself to the same deterministic line keys.
pub async fn migrate_items_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('print_jobs')")
            .fetch_all(pool)
            .await?;
    if !columns.iter().any(|c| c == "estimated_cost_cents") {
        return Ok(());
    }
    tracing::info!("Migrating print jobs to items");
    let mut tx = pool.begin().await?;
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(&mut *tx)
    .await?;
    for trigger in triggers {
        tx.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }
    // A column added to a table with rows needs a default; every row is rewritten below.
    for definition in [
        "note TEXT",
        "kind JSON NOT NULL DEFAULT '{\"kind\":\"standard\"}'",
        "items JSON NOT NULL DEFAULT '[]'",
        "price_cents INTEGER NOT NULL DEFAULT 0",
        "cost_cents INTEGER NOT NULL DEFAULT 0",
    ] {
        tx.execute(format!("ALTER TABLE print_jobs ADD COLUMN {definition}").as_str())
            .await?;
    }
    let rows = sqlx::query(
        "SELECT key, job_type, quantity, estimated_cost_cents, material_cost_cents \
         FROM print_jobs",
    )
    .fetch_all(&mut *tx)
    .await?;
    for row in rows {
        let key: String = row.try_get("key")?;
        let job_type: String = row.try_get("job_type")?;
        let migrated = legacy_items(LegacyPrintJob {
            key: &key,
            job_type: &job_type,
            quantity: row.try_get("quantity")?,
            estimated_cost_cents: row.try_get("estimated_cost_cents")?,
            material_cost_cents: row.try_get("material_cost_cents")?,
        })
        .map_err(protocol)?;
        sqlx::query(
            "UPDATE print_jobs SET kind = ?, items = ?, price_cents = ?, cost_cents = ? \
             WHERE key = ?",
        )
        .bind(kind_json())
        .bind(serde_json::to_string(&migrated.items).map_err(protocol)?)
        .bind(migrated.price_cents)
        .bind(migrated.cost_cents)
        .bind(&key)
        .execute(&mut *tx)
        .await?;
    }
    tx.execute("ALTER TABLE print_jobs DROP COLUMN estimated_cost_cents")
        .await?;
    tx.execute("ALTER TABLE print_jobs DROP COLUMN material_cost_cents")
        .await?;
    tx.commit().await
}

/// A whole number stored as either integer width; `None` when the field is absent or null.
fn whole_number(document: &Document, field: &str) -> AppResult<Option<i64>> {
    match document.get(field) {
        None | Some(Bson::Null) => Ok(None),
        Some(Bson::Int64(value)) => Ok(Some(*value)),
        Some(Bson::Int32(value)) => Ok(Some(i64::from(*value))),
        Some(other) => Err(AppError::internal(format!(
            "print_jobs.{field} is not a whole number: {other}"
        ))),
    }
}

/// A whole number every old print job has.
fn required_number(document: &Document, field: &str) -> AppResult<i64> {
    whole_number(document, field)?
        .ok_or_else(|| AppError::internal(format!("print_jobs.{field} is missing")))
}

fn text<'a>(document: &'a Document, field: &str) -> AppResult<&'a str> {
    document
        .get_str(field)
        .map_err(|err| AppError::internal(format!("print_jobs.{field}: {err}")))
}

/// Moves every shop's print jobs on a MongoDB database to lines (soft-deleted jobs included): a
/// job without `items` gets the lines, kind and totals its old amounts describe, and the old
/// fields are removed. Idempotent: a job that already has lines is left as it is.
pub async fn migrate_items_mongo(db: &Database, mode: Mode) -> AppResult<ItemsReport> {
    let print_jobs = db.collection::<Document>("print_jobs");
    let mut report = ItemsReport::default();
    let mut cursor = print_jobs
        .find(doc! { "items": { "$exists": false } })
        .await?;
    while let Some(job) = cursor.try_next().await? {
        let migrated = legacy_items(LegacyPrintJob {
            key: text(&job, "key")?,
            job_type: text(&job, "job_type")?,
            quantity: required_number(&job, "quantity")?,
            estimated_cost_cents: required_number(&job, "estimated_cost_cents")?,
            material_cost_cents: whole_number(&job, "material_cost_cents")?,
        })?;
        report.print_jobs += 1;
        if !migrated.items.is_empty() {
            report.with_items += 1;
        }
        if mode == Mode::Apply {
            let id = job
                .get_object_id("_id")
                .map_err(|err| AppError::internal(format!("print_jobs._id: {err}")))?;
            print_jobs
                .update_one(
                    doc! { "_id": id, "items": { "$exists": false } },
                    doc! {
                        "$set": {
                            "kind": mongodb::bson::serialize_to_bson(&PrintJobKind::Standard)?,
                            "items": mongodb::bson::serialize_to_bson(&migrated.items)?,
                            "price_cents": migrated.price_cents,
                            "cost_cents": migrated.cost_cents,
                        },
                        "$unset": { "estimated_cost_cents": "", "material_cost_cents": "" },
                    },
                )
                .await?;
        }
    }
    Ok(report)
}
