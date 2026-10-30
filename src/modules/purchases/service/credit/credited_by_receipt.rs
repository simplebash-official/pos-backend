use super::live_credits::live_credits;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::purchases::repository;

use std::collections::HashMap;

/// What is currently credited against each receipt: its credits, with voided ones left out.
pub(crate) async fn credited_by_receipt(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<HashMap<String, i64>> {
    let rows = repository::credit::list_credits_for_receipts(db, receipt_keys).await?;
    Ok(live_credits(&rows)
        .into_iter()
        .filter_map(|c| c.receipt_key.clone().map(|key| (key, c.amount_cents)))
        .fold(HashMap::new(), |mut totals, (key, amount)| {
            *totals.entry(key).or_insert(0) += amount;
            totals
        }))
}
