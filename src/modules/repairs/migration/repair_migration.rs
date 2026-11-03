use super::legacy_repair::{MigratedItems, legacy_items};
use crate::core::error::{AppError, AppResult};
use crate::domain::repairs::RepairKind;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use sqlx::{Executor, Row, SqlitePool};

/// What a run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct Report {
    /// Jobs given their items.
    pub repairs: u64,
    /// Of those, jobs that had a price or a material cost and so got an item.
    pub with_items: u64,
}

fn protocol(err: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Protocol(err.to_string())
}

fn kind_json() -> String {
    serde_json::to_string(&RepairKind::Standard).expect("a repair kind serializes to JSON")
}

/// Moves an old SQLite database's repair jobs to items, once: each job's quote and material
/// cost become its items (`legacy_items`), and the old columns are dropped. Does nothing once
/// `repairs.estimated_cost_cents` is gone. The sync triggers name columns, so they are dropped
/// first (`install_triggers` puts them back) and none of this is uploaded: the cloud migrates
/// itself to the same deterministic item keys.
pub async fn migrate_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('repairs')")
        .fetch_all(pool)
        .await?;
    if !columns.iter().any(|c| c == "estimated_cost_cents") {
        return Ok(());
    }
    tracing::info!("Migrating repair jobs to items");
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(pool)
    .await?;
    for trigger in triggers {
        pool.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }
    // A column added to a table with rows needs a default; every row is rewritten below.
    for (column, definition) in [
        ("kind", "JSON NOT NULL DEFAULT '{\"kind\":\"standard\"}'"),
        ("items", "JSON NOT NULL DEFAULT '[]'"),
        ("price_cents", "INTEGER NOT NULL DEFAULT 0"),
        ("cost_cents", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !columns.iter().any(|c| c == column) {
            pool.execute(format!("ALTER TABLE repairs ADD COLUMN {column} {definition}").as_str())
                .await?;
        }
    }
    let rows = sqlx::query(
        "SELECT key, issue_description, estimated_cost_cents, material_cost_cents FROM repairs",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        let key: String = row.get("key");
        let MigratedItems {
            items,
            price_cents,
            cost_cents,
        } = legacy_items(
            &key,
            &row.get::<String, _>("issue_description"),
            row.get("estimated_cost_cents"),
            row.get("material_cost_cents"),
        )
        .map_err(protocol)?;
        sqlx::query(
            "UPDATE repairs SET kind = ?, items = ?, price_cents = ?, cost_cents = ? WHERE key = ?",
        )
        .bind(kind_json())
        .bind(serde_json::to_string(&items).map_err(protocol)?)
        .bind(price_cents)
        .bind(cost_cents)
        .bind(&key)
        .execute(pool)
        .await?;
    }
    pool.execute("ALTER TABLE repairs DROP COLUMN estimated_cost_cents")
        .await?;
    pool.execute("ALTER TABLE repairs DROP COLUMN material_cost_cents")
        .await?;
    Ok(())
}

/// An old amount, stored as either integer width; absent or null when it was never set.
fn cents_of(document: &Document, field: &str) -> AppResult<Option<i64>> {
    match document.get(field) {
        None | Some(Bson::Null) => Ok(None),
        Some(Bson::Int64(cents)) => Ok(Some(*cents)),
        Some(Bson::Int32(cents)) => Ok(Some(i64::from(*cents))),
        Some(other) => Err(AppError::internal(format!(
            "repairs.{field} is not a whole number: {other}"
        ))),
    }
}

/// Moves every shop's repair jobs on a MongoDB database to items: a job without `items` gets
/// the items, kind and totals its old amounts describe, and the old fields are removed.
/// Idempotent: a job that already has items is left as it is.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> AppResult<Report> {
    let repairs = db.collection::<Document>("repairs");
    let mut report = Report::default();
    let mut cursor = repairs.find(doc! { "items": { "$exists": false } }).await?;
    while let Some(repair) = cursor.try_next().await? {
        let key = repair
            .get_str("key")
            .map_err(|e| AppError::internal(format!("repairs.key: {e}")))?;
        let issue = repair
            .get_str("issue_description")
            .map_err(|e| AppError::internal(format!("repairs.issue_description: {e}")))?;
        let migrated = legacy_items(
            key,
            issue,
            cents_of(&repair, "estimated_cost_cents")?,
            cents_of(&repair, "material_cost_cents")?,
        )?;
        report.repairs += 1;
        if !migrated.items.is_empty() {
            report.with_items += 1;
        }
        if mode == Mode::Apply {
            let id = repair
                .get_object_id("_id")
                .map_err(|e| AppError::internal(format!("repairs._id: {e}")))?;
            repairs
                .update_one(
                    doc! { "_id": id, "items": { "$exists": false } },
                    doc! {
                        "$set": {
                            "kind": mongodb::bson::serialize_to_bson(&RepairKind::Standard)?,
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
