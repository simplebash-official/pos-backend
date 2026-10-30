use super::enum_text::enum_text;
use super::job_columns::JOB_COLUMNS;
use super::job_err::job_err;
use super::job_filter::JobFilter;
use super::jobs::JOBS;
use super::row_to_job::row_to_job;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;

use crate::modules::labels::model::LabelPrintJobDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

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
