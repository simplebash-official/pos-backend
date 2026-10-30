use crate::clients::db::Db;
use crate::core::constants::prefixes::CASH_SETTINGS_KEY;
use crate::core::error::{AppError, AppResult};
use crate::modules::cash_settings::model::CashSettingsDocument;
use crate::modules::cash_settings::repository;

pub(super) async fn document(db: &Db) -> AppResult<CashSettingsDocument> {
    repository::find(db, CASH_SETTINGS_KEY)
        .await?
        .ok_or_else(|| {
            AppError::internal("The drawer rules have not been set up for this shop".to_string())
        })
}
