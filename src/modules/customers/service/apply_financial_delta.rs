use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::customers::repository;

/// Cross-module customer financial adjustment — called by billing/invoices/payments modules.
#[allow(dead_code)]
pub(crate) async fn apply_financial_delta(
    db: &Db,
    key: &str,
    purchases_delta: i64,
    balance_delta: i64,
) -> AppResult<()> {
    repository::adjust_customer_financials(db, key, purchases_delta, balance_delta)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
        })?;
    Ok(())
}
