use super::document::document;
use super::dto::dto;
use super::validate::validate;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::core::logging::domain::tracked;
use crate::domain::cash_settings::{CashSettings, UpdateCashSettingsRequest};
use crate::modules::cash_settings::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Replaces the drawer rules after checking them.
pub async fn update_settings(
    db: &Db,
    set_by_name: String,
    body: UpdateCashSettingsRequest,
) -> AppResult<CashSettings> {
    tracked("cash_settings.updated", async move {
        validate(&body)?;
        let mut doc = document(db).await?;
        doc.undo_window_seconds = body.undo_window_seconds;
        doc.approval_threshold_cents = body.approval_threshold_cents;
        doc.quick_amounts_cents = body.quick_amounts_cents;
        doc.float_target_cents = body.float_target_cents;
        doc.updated_by_name_snapshot = Some(set_by_name);
        doc.version += 1;
        doc.updated_at = BsonDateTime::now();
        repository::save(db, &doc).await?;
        Ok(dto(&doc))
    })
    .await
}
