// Dual-engine (Mongo / SQLite) access for the single `cash_settings` row.

use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
    },
    core::error::{AppError, AppResult},
    modules::cash_settings::model::CashSettingsDocument,
};

fn settings(db: &TenantDatabase) -> ScopedCollection<CashSettingsDocument> {
    db.collection("cash_settings")
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<CashSettingsDocument> {
    let chips: String = row.get("quick_amounts_cents");
    Ok(CashSettingsDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        undo_window_seconds: row.get("undo_window_seconds"),
        approval_threshold_cents: row.get("approval_threshold_cents"),
        quick_amounts_cents: serde_json::from_str(&chips)
            .map_err(|e| AppError::internal(format!("Unreadable quick amounts in database: {e}")))?,
        float_target_cents: row.get("float_target_cents"),
        updated_by_name_snapshot: row.get("updated_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// The shop's drawer rules, if the row exists.
pub(crate) async fn find(db: &Db, key: &str) -> AppResult<Option<CashSettingsDocument>> {
    match db {
        Db::Mongo(db) => Ok(settings(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM cash_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}

/// Inserts the row.
pub(crate) async fn insert(db: &Db, mut document: CashSettingsDocument) -> AppResult<CashSettingsDocument> {
    match db {
        Db::Mongo(db) => {
            let res = settings(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map(|o| o.to_hex()).unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO cash_settings (
                    key, id, undo_window_seconds, approval_threshold_cents, quick_amounts_cents,
                    float_target_cents, updated_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.undo_window_seconds)
            .bind(document.approval_threshold_cents)
            .bind(serde_json::to_string(&document.quick_amounts_cents).expect("integers serialize"))
            .bind(document.float_target_cents)
            .bind(&document.updated_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(pool)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// Writes changed rules back (the service bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &CashSettingsDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            settings(db).replace_one(doc! { "key": &document.key }, document).await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE cash_settings SET undo_window_seconds = ?, approval_threshold_cents = ?,
                    quick_amounts_cents = ?, float_target_cents = ?, updated_by_name_snapshot = ?,
                    version = ?, updated_at = ? WHERE key = ?",
            )
            .bind(document.undo_window_seconds)
            .bind(document.approval_threshold_cents)
            .bind(serde_json::to_string(&document.quick_amounts_cents).expect("integers serialize"))
            .bind(document.float_target_cents)
            .bind(&document.updated_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
