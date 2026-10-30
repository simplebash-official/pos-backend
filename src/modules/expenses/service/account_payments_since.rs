use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::expenses::PaidFrom;
use crate::modules::expenses::repository;

/// What was paid out of the bank, a card or online on or after `since` (`YYYY-MM-DD`), as `(account, cents)`
/// per payment. Drawer payments are cash movements and not counted here.
pub(crate) async fn account_payments_since(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(PaidFrom, i64)>> {
    Ok(repository::list_all_payments(db)
        .await?
        .into_iter()
        .filter(|p| p.paid_on.as_str() >= since)
        .map(|p| (p.paid_from, p.amount_cents))
        .collect())
}
