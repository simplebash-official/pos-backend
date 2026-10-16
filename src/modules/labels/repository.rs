// Mongo and SQLite access for the `label_templates` and `label_print_jobs`
// collections/tables. Like every repository it never turns a miss into an
// error: `service` decides what a missing template or job means.

use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, to_bson_datetime},
    },
    core::error::{AppError, AppResult},
    domain::labels::{LabelElement, LabelJobOutcome, LabelPrintAction, LabelPrintMethod},
    modules::labels::model::{LabelJobItemDocument, LabelPrintJobDocument, LabelTemplateDocument},
};

const COLLECTION: &str = "label_templates";
const JOBS: &str = "label_print_jobs";

fn store_err(action: &str, e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("Failed to {action} label template: {e}"))
}

fn row_to_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<LabelTemplateDocument> {
    let elements_json: String = row.get("elements");
    let elements: Vec<LabelElement> = serde_json::from_str(&elements_json)
        .map_err(|e| AppError::internal(format!("Stored label template is corrupt: {e}")))?;
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(LabelTemplateDocument {
        id: None,
        key: row.get("key"),
        name: row.get("name"),
        width_mm: row.get("width_mm"),
        height_mm: row.get("height_mm"),
        elements,
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at),
        updated_at: to_bson_datetime(&updated_at),
    })
}

/// Every template the shop built, ordered by name.
pub(crate) async fn list_templates(db: &Db) -> AppResult<Vec<LabelTemplateDocument>> {
    match db {
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT key, name, width_mm, height_mm, elements, version, created_at, updated_at \
                 FROM label_templates ORDER BY name COLLATE NOCASE",
            )
            .fetch_all(pool)
            .await
            .map_err(|e| store_err("list", e))?;
            rows.iter().map(row_to_doc).collect()
        }
        Db::Mongo(mongo) => {
            let mut templates: Vec<LabelTemplateDocument> = mongo
                .collection::<LabelTemplateDocument>(COLLECTION)
                .find(doc! {})
                .await?
                .try_collect()
                .await?;
            templates.sort_by_key(|t| t.name.to_lowercase());
            Ok(templates)
        }
    }
}

/// One template by its `ltpl_` key.
pub(crate) async fn find_template_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<LabelTemplateDocument>> {
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT key, name, width_mm, height_mm, elements, version, created_at, updated_at \
                 FROM label_templates WHERE key = ?1 LIMIT 1",
            )
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|e| store_err("fetch", e))?;
            row.as_ref().map(row_to_doc).transpose()
        }
        Db::Mongo(mongo) => Ok(mongo
            .collection::<LabelTemplateDocument>(COLLECTION)
            .find_one(doc! { "key": key })
            .await?),
    }
}

/// Inserts a new template.
pub(crate) async fn insert_template(db: &Db, template: &LabelTemplateDocument) -> AppResult<()> {
    match db {
        Db::Sqlite(pool) => {
            let elements =
                serde_json::to_string(&template.elements).map_err(|e| store_err("save", e))?;
            sqlx::query(
                "INSERT INTO label_templates \
                 (key, name, width_mm, height_mm, elements, version, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(&template.key)
            .bind(&template.name)
            .bind(template.width_mm)
            .bind(template.height_mm)
            .bind(&elements)
            .bind(template.version)
            .bind(bson_to_iso(&template.created_at))
            .bind(bson_to_iso(&template.updated_at))
            .execute(pool)
            .await
            .map_err(|e| store_err("save", e))?;
            Ok(())
        }
        Db::Mongo(mongo) => {
            mongo
                .collection::<LabelTemplateDocument>(COLLECTION)
                .insert_one(template)
                .await?;
            Ok(())
        }
    }
}

/// Replaces a template's design and bumps its version; `None` when the key
/// does not exist.
pub(crate) async fn update_template(
    db: &Db,
    key: &str,
    name: &str,
    width_mm: f64,
    height_mm: f64,
    elements: &[LabelElement],
) -> AppResult<Option<LabelTemplateDocument>> {
    let now = BsonDateTime::now();
    match db {
        Db::Sqlite(pool) => {
            let elements_json =
                serde_json::to_string(elements).map_err(|e| store_err("save", e))?;
            let result = sqlx::query(
                "UPDATE label_templates SET name = ?2, width_mm = ?3, height_mm = ?4, \
                 elements = ?5, version = version + 1, updated_at = ?6 WHERE key = ?1",
            )
            .bind(key)
            .bind(name)
            .bind(width_mm)
            .bind(height_mm)
            .bind(&elements_json)
            .bind(bson_to_iso(&now))
            .execute(pool)
            .await
            .map_err(|e| store_err("save", e))?;
            if result.rows_affected() == 0 {
                return Ok(None);
            }
            find_template_by_key(db, key).await
        }
        Db::Mongo(mongo) => {
            let elements_bson =
                mongodb::bson::serialize_to_bson(elements).map_err(|e| store_err("save", e))?;
            Ok(mongo
                .collection::<LabelTemplateDocument>(COLLECTION)
                .find_one_and_update(
                    doc! { "key": key },
                    doc! {
                        "$set": {
                            "name": name,
                            "width_mm": width_mm,
                            "height_mm": height_mm,
                            "elements": elements_bson,
                            "updated_at": now,
                        },
                        "$inc": { "version": 1 },
                    },
                )
                .return_document(mongodb::options::ReturnDocument::After)
                .await?)
        }
    }
}

/// Deletes a template; `false` when the key does not exist.
pub(crate) async fn delete_template(db: &Db, key: &str) -> AppResult<bool> {
    match db {
        Db::Sqlite(pool) => {
            let result = sqlx::query("DELETE FROM label_templates WHERE key = ?1")
                .bind(key)
                .execute(pool)
                .await
                .map_err(|e| store_err("delete", e))?;
            Ok(result.rows_affected() > 0)
        }
        Db::Mongo(mongo) => Ok(mongo
            .collection::<LabelTemplateDocument>(COLLECTION)
            .delete_one(doc! { "key": key })
            .await?
            .deleted_count
            > 0),
    }
}

// ============================================================================
// Print jobs
// ============================================================================

fn job_err(action: &str, e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("Failed to {action} label print job: {e}"))
}

/// An enum's stored text (its serde name), shared by SQLite columns and filters.
fn enum_text<T: serde::Serialize>(value: &T) -> AppResult<String> {
    match serde_json::to_value(value).map_err(|e| job_err("save", e))? {
        serde_json::Value::String(text) => Ok(text),
        other => Err(job_err("save", format!("unexpected value {other}"))),
    }
}

fn text_enum<T: serde::de::DeserializeOwned>(text: &str) -> AppResult<T> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
        .map_err(|e| AppError::internal(format!("Stored label print job is corrupt: {e}")))
}

const JOB_COLUMNS: &str = "key, template_key, template_name, width_mm, height_mm, action, method, \
     printer_name, dpi, outcome, error, printed_by_user_id, printed_by_name, items, total_labels, \
     created_at, updated_at";

fn row_to_job(row: &sqlx::sqlite::SqliteRow) -> AppResult<LabelPrintJobDocument> {
    let items_json: String = row.get("items");
    let items: Vec<LabelJobItemDocument> = serde_json::from_str(&items_json)
        .map_err(|e| AppError::internal(format!("Stored label print job is corrupt: {e}")))?;
    let method: Option<String> = row.get("method");
    let dpi: Option<i64> = row.get("dpi");
    let total_labels: i64 = row.get("total_labels");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(LabelPrintJobDocument {
        id: None,
        key: row.get("key"),
        template_key: row.get("template_key"),
        template_name: row.get("template_name"),
        width_mm: row.get("width_mm"),
        height_mm: row.get("height_mm"),
        action: text_enum::<LabelPrintAction>(&row.get::<String, _>("action"))?,
        method: method
            .as_deref()
            .map(text_enum::<LabelPrintMethod>)
            .transpose()?,
        printer_name: row.get("printer_name"),
        dpi: dpi.map(|dpi| dpi as u32),
        outcome: text_enum::<LabelJobOutcome>(&row.get::<String, _>("outcome"))?,
        error: row.get("error"),
        printed_by_user_id: row.get("printed_by_user_id"),
        printed_by_name: row.get("printed_by_name"),
        items,
        total_labels: total_labels as u32,
        created_at: to_bson_datetime(&created_at),
        updated_at: to_bson_datetime(&updated_at),
    })
}

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

/// Filters of a job listing, already validated by `service`.
pub(crate) struct JobFilter {
    pub from: Option<BsonDateTime>,
    pub to: Option<BsonDateTime>,
    pub product_key: Option<String>,
    pub action: Option<LabelPrintAction>,
}

/// One page of jobs matching `filter`, newest first, plus the total count.
pub(crate) async fn list_jobs(
    db: &Db,
    filter: &JobFilter,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<LabelPrintJobDocument>, u64)> {
    match db {
        Db::Sqlite(pool) => {
            // Fixed clause text with numbered binds; absent filters bind NULL and drop out.
            let conditions = "(?1 IS NULL OR created_at >= ?1) AND (?2 IS NULL OR created_at < ?2) \
                 AND (?3 IS NULL OR action = ?3) \
                 AND (?4 IS NULL OR EXISTS (SELECT 1 FROM json_each(label_print_jobs.items) \
                      WHERE json_extract(json_each.value, '$.product_key') = ?4))";
            let from = filter.from.as_ref().map(bson_to_iso);
            let to = filter.to.as_ref().map(bson_to_iso);
            let action = filter.action.as_ref().map(enum_text).transpose()?;
            let total: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*) FROM label_print_jobs WHERE {conditions}"
            ))
            .bind(&from)
            .bind(&to)
            .bind(&action)
            .bind(&filter.product_key)
            .fetch_one(pool)
            .await
            .map_err(|e| job_err("list", e))?;
            let rows = sqlx::query(&format!(
                "SELECT {JOB_COLUMNS} FROM label_print_jobs WHERE {conditions} \
                 ORDER BY created_at DESC, key DESC LIMIT ?5 OFFSET ?6"
            ))
            .bind(&from)
            .bind(&to)
            .bind(&action)
            .bind(&filter.product_key)
            .bind(limit as i64)
            .bind(skip as i64)
            .fetch_all(pool)
            .await
            .map_err(|e| job_err("list", e))?;
            Ok((
                rows.iter().map(row_to_job).collect::<AppResult<_>>()?,
                total as u64,
            ))
        }
        Db::Mongo(mongo) => {
            let mut query = doc! {};
            let mut created = doc! {};
            if let Some(from) = filter.from {
                created.insert("$gte", from);
            }
            if let Some(to) = filter.to {
                created.insert("$lt", to);
            }
            if !created.is_empty() {
                query.insert("created_at", created);
            }
            if let Some(action) = &filter.action {
                query.insert("action", enum_text(action)?);
            }
            if let Some(product_key) = &filter.product_key {
                query.insert("items.product_key", product_key);
            }
            let collection = mongo.collection::<LabelPrintJobDocument>(JOBS);
            let total = collection.count_documents(query.clone()).await?;
            let jobs = collection
                .find(query)
                .sort(doc! { "created_at": -1, "key": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?
                .try_collect()
                .await?;
            Ok((jobs, total))
        }
    }
}

/// One sticker group of a job that counts as labelled (anything not failed).
pub(crate) struct LabelledEntry {
    pub job_key: String,
    pub product_key: String,
    pub copies: u32,
    pub created_at: BsonDateTime,
}

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
