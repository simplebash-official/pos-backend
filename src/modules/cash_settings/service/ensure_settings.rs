use crate::clients::db::Db;
use crate::core::constants::prefixes::CASH_SETTINGS_KEY;
use crate::core::error::AppResult;
use crate::domain::cash_settings::{STARTING_QUICK_AMOUNTS_CENTS, STARTING_UNDO_WINDOW_SECONDS};
use crate::modules::cash_settings::model::CashSettingsDocument;
use crate::modules::cash_settings::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Creates the shop's drawer rules with the shipped starting values when the row is missing. Never
/// touches an existing row, so the owner's edits survive a re-run.
pub async fn ensure_settings(db: &Db) -> AppResult<()> {
    if repository::find(db, CASH_SETTINGS_KEY).await?.is_some() {
        return Ok(());
    }
    // A fixed time, like the built-in categories, so the same row made on two devices is one row for sync.
    let seeded_at = BsonDateTime::from_millis(1_767_225_600_000);
    repository::insert(
        db,
        CashSettingsDocument {
            id: None,
            key: CASH_SETTINGS_KEY.to_string(),
            undo_window_seconds: STARTING_UNDO_WINDOW_SECONDS,
            approval_threshold_cents: None,
            quick_amounts_cents: STARTING_QUICK_AMOUNTS_CENTS.to_vec(),
            float_target_cents: None,
            updated_by_name_snapshot: None,
            version: 1,
            created_at: seeded_at,
            updated_at: seeded_at,
            updated_by_device: None,
        },
    )
    .await?;
    Ok(())
}
