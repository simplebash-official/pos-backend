use super::import_batches::import_batches;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::imports::model::ImportBatchDocument;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_batch(
    db: &Db,
    mut document: ImportBatchDocument,
) -> AppResult<ImportBatchDocument> {
    match db {
        Db::Mongo(db) => {
            let result = import_batches(db).insert_one(&document).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id_hex = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            let errors_json =
                serde_json::to_string(&document.errors).unwrap_or_else(|_| "[]".to_string());
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let processed_rows = (document.successful_rows + document.failed_rows) as i64;

            sqlx::query(
                r#"INSERT INTO import_batches (
                    key, id, file_name, file_type, file_size_bytes, target, status,
                    total_rows, processed_rows, successful_rows, failed_rows, errors,
                    created_by_user_key, created_by_user_name, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(&document.key)
            .bind(&id_hex)
            .bind(&document.file_name)
            .bind(&document.file_type)
            .bind(document.file_size_bytes)
            .bind(&document.target)
            .bind(&document.status)
            .bind(document.total_rows as i64)
            .bind(processed_rows)
            .bind(document.successful_rows as i64)
            .bind(document.failed_rows as i64)
            .bind(&errors_json)
            .bind(&document.created_by_user_key)
            .bind(&document.created_by_user_name)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id_hex).ok();
            Ok(document)
        }
    }
}
