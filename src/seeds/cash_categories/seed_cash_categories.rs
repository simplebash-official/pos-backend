use super::cash_category_seed_result::CashCategorySeedResult;
use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::modules::cash_categories::service;

/// Creates any shipped category the shop does not have yet.
pub async fn seed_cash_categories(db: &Db) -> AppResult<CashCategorySeedResult> {
    service::ensure_system_categories(db).await
}
