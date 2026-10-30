use super::document::document;
use crate::clients::db::Db;
use crate::core::error::AppResult;

/// The shop-wide payout limit that needs a manager PIN, when one is set.
pub(crate) async fn approval_threshold_cents(db: &Db) -> AppResult<Option<i64>> {
    Ok(document(db).await?.approval_threshold_cents)
}
