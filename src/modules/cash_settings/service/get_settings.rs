use super::document::document;
use super::dto::dto;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::cash_settings::CashSettings;

/// The shop's drawer rules.
pub async fn get_settings(db: &Db) -> AppResult<CashSettings> {
    Ok(dto(&document(db).await?))
}
