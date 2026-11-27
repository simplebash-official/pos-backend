//! Mongo and SQLite access for `print_frame_sizes`.

use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::clients::sqlite::generate_id_hex;
use crate::clients::sqlite::to_bson_datetime;
use crate::clients::tenant_db::ScopedCollection;
use crate::clients::tenant_db::TenantDatabase;
use crate::core::error::AppError;
use crate::core::error::AppResult;
use crate::modules::print_jobs::model::PrintFrameSizeDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

const TABLE: &str = "print_frame_sizes";

fn prices(db: &TenantDatabase) -> ScopedCollection<PrintFrameSizeDocument> {
    db.collection(TABLE)
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<PrintFrameSizeDocument> {
    Ok(PrintFrameSizeDocument {
        id: Some(
            ObjectId::parse_str(row.get::<String, _>("id"))
                .map_err(|e| AppError::internal(format!("frame size id: {e}")))?,
        ),
        key: row.get("key"),
        size: row.get("size"),
        width_hundredths_inch: row.get("width_hundredths_inch"),
        active: row.get::<i64, _>("active") != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at"))?,
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at"))?,
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Every frame size of the shop, by size text then width; screens order sizes by number.
pub(crate) async fn list(db: &Db) -> AppResult<Vec<PrintFrameSizeDocument>> {
    match db {
        Db::Mongo(db) => Ok(prices(db)
            .find(doc! {})
            .sort(doc! { "size": 1, "width_hundredths_inch": 1 })
            .await?
            .try_collect()
            .await?),
        Db::Sqlite(pool) => {
            let rows =
                sqlx::query("SELECT * FROM print_frame_sizes ORDER BY size, width_hundredths_inch")
                    .fetch_all(pool)
                    .await?;
            rows.iter().map(from_row).collect()
        }
    }
}

/// One frame size by its key.
pub(crate) async fn find(db: &Db, key: &str) -> AppResult<Option<PrintFrameSizeDocument>> {
    match db {
        Db::Mongo(db) => Ok(prices(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => sqlx::query("SELECT * FROM print_frame_sizes WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?
            .as_ref()
            .map(from_row)
            .transpose(),
    }
}

/// Stores a new frame size; returns it with its id.
pub(crate) async fn insert(
    db: &Db,
    mut document: PrintFrameSizeDocument,
) -> AppResult<PrintFrameSizeDocument> {
    match db {
        Db::Mongo(db) => {
            let result = prices(db).insert_one(&document).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map_or_else(generate_id_hex, |oid| oid.to_hex());
            sqlx::query(
                "INSERT INTO print_frame_sizes (key, id, size, width_hundredths_inch, \
                 active, version, created_at, updated_at, updated_by_device) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.size)
            .bind(document.width_hundredths_inch)
            .bind(i64::from(document.active))
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;
            document.id =
                Some(ObjectId::parse_str(&id).expect("generate_id_hex returns an ObjectId hex"));
            Ok(document)
        }
    }
}

/// Writes a changed frame size if nobody changed it since `expected_version`. False when they did.
pub(crate) async fn replace(
    db: &Db,
    document: &PrintFrameSizeDocument,
    expected_version: i64,
) -> AppResult<bool> {
    match db {
        Db::Mongo(db) => Ok(prices(db)
            .replace_one(
                doc! { "key": &document.key, "version": expected_version },
                document,
            )
            .await?
            .matched_count
            == 1),
        Db::Sqlite(pool) => Ok(sqlx::query(
            "UPDATE print_frame_sizes SET active = ?, version = ?, updated_at = ?, \
             updated_by_device = ? WHERE key = ? AND version = ?",
        )
        .bind(i64::from(document.active))
        .bind(document.version)
        .bind(bson_to_iso(&document.updated_at))
        .bind(&document.updated_by_device)
        .bind(&document.key)
        .bind(expected_version)
        .execute(pool)
        .await?
        .rows_affected()
            == 1),
    }
}
