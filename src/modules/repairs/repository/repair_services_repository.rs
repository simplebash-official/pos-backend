//! Mongo and SQLite access for `repair_service_templates`.

use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::{AppError, AppResult};
use crate::modules::repairs::model::RepairServiceDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

const TABLE: &str = "repair_service_templates";

fn services(db: &TenantDatabase) -> ScopedCollection<RepairServiceDocument> {
    db.collection(TABLE)
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<RepairServiceDocument> {
    let warranty: String = row.get("warranty");
    Ok(RepairServiceDocument {
        id: Some(
            ObjectId::parse_str(row.get::<String, _>("id"))
                .map_err(|e| AppError::internal(format!("repair service id: {e}")))?,
        ),
        key: row.get("key"),
        name: row.get("name"),
        labor_cents: row.get("labor_cents"),
        warranty: serde_json::from_str(&warranty).map_err(|e| {
            AppError::internal(format!(
                "stored repair service warranty is not readable: {e}"
            ))
        })?,
        active: row.get::<i64, _>("active") != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at"))?,
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at"))?,
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Every service of the shop, by name.
pub(crate) async fn list(db: &Db) -> AppResult<Vec<RepairServiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(services(db)
            .find(doc! {})
            .sort(doc! { "name": 1 })
            .await?
            .try_collect()
            .await?),
        Db::Sqlite(pool) => {
            let rows =
                sqlx::query("SELECT * FROM repair_service_templates ORDER BY name COLLATE NOCASE")
                    .fetch_all(pool)
                    .await?;
            rows.iter().map(from_row).collect()
        }
    }
}

/// One service by its key.
pub(crate) async fn find(db: &Db, key: &str) -> AppResult<Option<RepairServiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(services(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => sqlx::query("SELECT * FROM repair_service_templates WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?
            .as_ref()
            .map(from_row)
            .transpose(),
    }
}

/// Stores a new service; returns it with its id.
pub(crate) async fn insert(
    db: &Db,
    mut document: RepairServiceDocument,
) -> AppResult<RepairServiceDocument> {
    match db {
        Db::Mongo(db) => {
            let result = services(db).insert_one(&document).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map_or_else(generate_id_hex, |oid| oid.to_hex());
            sqlx::query(
                "INSERT INTO repair_service_templates (key, id, name, labor_cents, warranty, \
                 active, version, created_at, updated_at, updated_by_device) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(document.labor_cents)
            .bind(serde_json::to_string(&document.warranty).expect("a warranty serializes"))
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

/// Writes a changed service if nobody changed it since `expected_version`. False when they did.
pub(crate) async fn replace(
    db: &Db,
    document: &RepairServiceDocument,
    expected_version: i64,
) -> AppResult<bool> {
    match db {
        Db::Mongo(db) => Ok(services(db)
            .replace_one(
                doc! { "key": &document.key, "version": expected_version },
                document,
            )
            .await?
            .matched_count
            == 1),
        Db::Sqlite(pool) => Ok(sqlx::query(
            "UPDATE repair_service_templates SET name = ?, labor_cents = ?, warranty = ?, \
             active = ?, version = ?, updated_at = ?, updated_by_device = ? \
             WHERE key = ? AND version = ?",
        )
        .bind(&document.name)
        .bind(document.labor_cents)
        .bind(serde_json::to_string(&document.warranty).expect("a warranty serializes"))
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
