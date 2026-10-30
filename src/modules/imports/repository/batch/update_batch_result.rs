use super::find_batch_by_key::find_batch_by_key;
use super::import_batches::import_batches;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;
use crate::core::error::AppResult;
use crate::modules::imports::model::{ImportBatchDocument, ImportRowErrorDocument};
use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn update_batch_result(
    db: &Db,
    key: &str,
    successful_rows: u64,
    failed_rows: u64,
    status: &str,
    errors: Vec<ImportRowErrorDocument>,
) -> AppResult<Option<ImportBatchDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let bson_errors = errors
                .into_iter()
                .filter_map(|e| mongodb::bson::serialize_to_document(&e).ok())
                .map(mongodb::bson::Bson::Document)
                .collect::<Vec<_>>();

            let update = doc! {
                "$set": {
                    "successful_rows": successful_rows as i64,
                    "failed_rows": failed_rows as i64,
                    "status": status,
                    "errors": bson_errors,
                    "updated_at": now,
                }
            };

            let updated = import_batches(db)
                .find_one_and_update(doc! { "key": key }, update)
                .return_document(ReturnDocument::After)
                .await?;

            Ok(updated)
        }
        Db::Sqlite(pool) => {
            let updated_at_iso = now_utc_iso();
            let errors_json = serde_json::to_string(&errors).unwrap_or_else(|_| "[]".to_string());
            let processed_rows = (successful_rows + failed_rows) as i64;

            sqlx::query(
                r#"UPDATE import_batches SET
                    successful_rows = ?,
                    failed_rows = ?,
                    processed_rows = ?,
                    status = ?,
                    errors = ?,
                    updated_at = ?
                WHERE key = ?"#,
            )
            .bind(successful_rows as i64)
            .bind(failed_rows as i64)
            .bind(processed_rows)
            .bind(status)
            .bind(&errors_json)
            .bind(&updated_at_iso)
            .bind(key)
            .execute(pool)
            .await?;

            find_batch_by_key(db, key).await
        }
    }
}
