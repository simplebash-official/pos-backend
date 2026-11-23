//! Mongo and SQLite access for `warranty_condition_templates`.

use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::{AppError, AppResult};
use crate::modules::warranties::model::WarrantyConditionDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

const TABLE: &str = "warranty_condition_templates";

fn conditions_collection(db: &TenantDatabase) -> ScopedCollection<WarrantyConditionDocument> {
    db.collection(TABLE)
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<WarrantyConditionDocument> {
    Ok(WarrantyConditionDocument {
        id: Some(
            ObjectId::parse_str(row.get::<String, _>("id"))
                .map_err(|e| AppError::internal(format!("condition template id: {e}")))?,
        ),
        key: row.get("key"),
        name: row.get("name"),
        active: row.get::<i64, _>("active") != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at"))?,
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at"))?,
    })
}

/// Active condition templates of the shop, by name.
pub(crate) async fn list_active(db: &Db) -> AppResult<Vec<WarrantyConditionDocument>> {
    match db {
        Db::Mongo(db) => Ok(conditions_collection(db)
            .find(doc! { "active": true })
            .sort(doc! { "name": 1 })
            .await?
            .try_collect()
            .await?),
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM warranty_condition_templates WHERE active != 0 ORDER BY name COLLATE NOCASE",
            )
            .fetch_all(pool)
            .await?;
            rows.iter().map(from_row).collect()
        }
    }
}

/// Finds one condition template by key.
pub(crate) async fn find_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<WarrantyConditionDocument>> {
    match db {
        Db::Mongo(db) => Ok(conditions_collection(db)
            .find_one(doc! { "key": key })
            .await?),
        Db::Sqlite(pool) => sqlx::query("SELECT * FROM warranty_condition_templates WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?
            .as_ref()
            .map(from_row)
            .transpose(),
    }
}

/// Stores a new condition template.
pub(crate) async fn insert(
    db: &Db,
    mut document: WarrantyConditionDocument,
) -> AppResult<WarrantyConditionDocument> {
    match db {
        Db::Mongo(db) => {
            let result = conditions_collection(db).insert_one(&document).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map_or_else(generate_id_hex, |oid| oid.to_hex());
            sqlx::query(
                "INSERT INTO warranty_condition_templates (key, id, name, active, version, \
                 created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(i64::from(document.active))
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(pool)
            .await?;
            document.id =
                Some(ObjectId::parse_str(&id).expect("generate_id_hex returns an ObjectId hex"));
            Ok(document)
        }
    }
}

/// Replaces an existing condition template if version matches.
pub(crate) async fn replace(
    db: &Db,
    document: &WarrantyConditionDocument,
    expected_version: i64,
) -> AppResult<bool> {
    match db {
        Db::Mongo(db) => Ok(conditions_collection(db)
            .replace_one(
                doc! { "key": &document.key, "version": expected_version },
                document,
            )
            .await?
            .matched_count
            == 1),
        Db::Sqlite(pool) => Ok(sqlx::query(
            "UPDATE warranty_condition_templates SET name = ?, active = ?, version = ?, \
             updated_at = ? WHERE key = ? AND version = ?",
        )
        .bind(&document.name)
        .bind(i64::from(document.active))
        .bind(document.version)
        .bind(bson_to_iso(&document.updated_at))
        .bind(&document.key)
        .bind(expected_version)
        .execute(pool)
        .await?
        .rows_affected()
            == 1),
    }
}
