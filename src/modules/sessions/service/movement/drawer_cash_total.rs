use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::repository::drawer;

/// Cash the shop holds in all its drawers right now (the sum of every terminal's running balance).
pub(crate) async fn drawer_cash_total(db: &Db) -> AppResult<i64> {
    Ok(drawer::list_drawer_states(db)
        .await?
        .iter()
        .map(|s| s.current_balance_cents)
        .sum())
}
