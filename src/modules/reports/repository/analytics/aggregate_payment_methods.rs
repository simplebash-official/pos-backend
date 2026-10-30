use super::super::sales;

use crate::clients::db::Db;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};

// ---------------------------------------------------------------------------
// payment methods (range-scoped)
// ---------------------------------------------------------------------------
pub(crate) async fn aggregate_payment_methods(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<(crate::domain::reports::PaymentMethodBreakdown, i64)> {
    let breakdown = sales::compute_payment_methods(db, start, end).await?;
    let total = breakdown.cash_cents
        + breakdown.card_cents
        + breakdown.online_cents
        + breakdown.credit_cents;
    Ok((breakdown, total))
}
