use super::range::range;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{AnalyticsPaymentMethodsResponse, AnalyticsRangeQuery};
use crate::modules::reports::repository;

// ---------------------------------------------------------------------------
// payment methods
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_payment_methods(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<AnalyticsPaymentMethodsResponse> {
    let (start, end) = range(&query)?;
    let (breakdown, total_cents) =
        repository::analytics::aggregate_payment_methods(db, start, end).await?;
    Ok(AnalyticsPaymentMethodsResponse {
        period_start: start,
        period_end: end,
        breakdown,
        total_cents,
    })
}
