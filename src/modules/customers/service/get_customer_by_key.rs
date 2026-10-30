use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::customers::Customer;

use crate::modules::customers::repository;

/// Cross-module customer lookup by key — called by billing/invoices/repairs modules.
#[allow(dead_code)]
pub(crate) async fn get_customer_by_key(db: &Db, key: &str) -> AppResult<Customer> {
    let doc = repository::find_customer_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
        })?;
    Ok(doc.into_customer())
}
