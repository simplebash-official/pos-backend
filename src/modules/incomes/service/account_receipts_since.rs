use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::incomes::ReceivedInto;
use crate::modules::incomes::repository;

pub(crate) async fn account_receipts_since(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(ReceivedInto, i64)>> {
    Ok(repository::list_incomes(db, Some(since), None)
        .await?
        .into_iter()
        .map(|i| (i.received_into, i.amount_cents))
        .collect())
}
