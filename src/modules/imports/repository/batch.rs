// Mongo and SQLite access for the `import_batches` collection / table.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{DateTime as BsonDateTime, doc, oid::ObjectId},
    options::ReturnDocument,
};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, now_utc_iso, to_bson_datetime},
    },
    core::error::AppResult,
    modules::imports::model::{ImportBatchDocument, ImportRowErrorDocument},
};

fn import_batches(db: &TenantDatabase) -> ScopedCollection<ImportBatchDocument> {
    db.collection("import_batches")
}

fn map_sqlite_row_to_batch(row: &sqlx::sqlite::SqliteRow) -> AppResult<ImportBatchDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let errors_str: Option<String> = row.get("errors");
    let errors: Vec<ImportRowErrorDocument> = match errors_str {
        Some(s) if !s.is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    };
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    let total_rows_i: i64 = row.get("total_rows");
    let successful_rows_i: i64 = row.get("successful_rows");
    let failed_rows_i: i64 = row.get("failed_rows");

    Ok(ImportBatchDocument {
        id,
        key: row.get("key"),
        target: row.get("target"),
        file_name: row.get("file_name"),
        file_type: row.get("file_type"),
        file_size_bytes: row.get("file_size_bytes"),
        total_rows: total_rows_i as u64,
        successful_rows: successful_rows_i as u64,
        failed_rows: failed_rows_i as u64,
        status: row.get("status"),
        errors,
        created_by_user_key: row.get("created_by_user_key"),
        created_by_user_name: row.get("created_by_user_name"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}

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

pub(crate) async fn find_batch_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<ImportBatchDocument>> {
    match db {
        Db::Mongo(db) => Ok(import_batches(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM import_batches WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.map(|r| map_sqlite_row_to_batch(&r)).transpose()
        }
    }
}

pub(crate) async fn list_batches(
    db: &Db,
    target: Option<&str>,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<ImportBatchDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut filter = doc! {};
            if let Some(t) = target.filter(|s| !s.is_empty()) {
                filter.insert("target", t);
            }

            let total = import_batches(db).count_documents(filter.clone()).await?;

            let mut cursor = import_batches(db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut items = Vec::new();
            while let Some(doc) = cursor.try_next().await? {
                items.push(doc);
            }

            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let (where_clause, bind_target) = match target.filter(|s| !s.is_empty()) {
                Some(t) => ("WHERE target = ?", Some(t)),
                None => ("", None),
            };

            let count_query_str = format!("SELECT COUNT(*) FROM import_batches {where_clause}");
            let mut count_query = sqlx::query_scalar::<_, i64>(&count_query_str);
            if let Some(t) = bind_target {
                count_query = count_query.bind(t);
            }
            let total: i64 = count_query.fetch_one(pool).await?;

            let list_query_str = format!(
                "SELECT * FROM import_batches {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut list_query = sqlx::query(&list_query_str);
            if let Some(t) = bind_target {
                list_query = list_query.bind(t);
            }
            let rows = list_query
                .bind(limit as i64)
                .bind(skip as i64)
                .fetch_all(pool)
                .await?;

            let mut items = Vec::new();
            for row in rows {
                items.push(map_sqlite_row_to_batch(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}
