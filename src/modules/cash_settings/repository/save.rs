use super::settings::settings;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::modules::cash_settings::model::CashSettingsDocument;
use mongodb::bson::doc;

/// Writes changed rules back (the service bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &CashSettingsDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            settings(db)
                .replace_one(doc! { "key": &document.key }, document)
                .await?;
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
