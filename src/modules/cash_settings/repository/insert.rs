use super::settings::settings;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::cash_settings::model::CashSettingsDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts the row.
pub(crate) async fn insert(
    db: &Db,
    mut document: CashSettingsDocument,
) -> AppResult<CashSettingsDocument> {
    match db {
        Db::Mongo(db) => {
            let res = settings(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
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
