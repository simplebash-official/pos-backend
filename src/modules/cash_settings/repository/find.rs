use super::settings::settings;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::modules::cash_settings::model::CashSettingsDocument;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<CashSettingsDocument> {
    let chips: String = row.get("quick_amounts_cents");
    Ok(CashSettingsDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        undo_window_seconds: row.get("undo_window_seconds"),
        approval_threshold_cents: row.get("approval_threshold_cents"),
        quick_amounts_cents: serde_json::from_str(&chips).map_err(|e| {
            AppError::internal(format!("Unreadable quick amounts in database: {e}"))
        })?,
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
