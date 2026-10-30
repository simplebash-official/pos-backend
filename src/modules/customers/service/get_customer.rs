use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::customers::Customer;

use crate::modules::customers::repository;

pub async fn get_customer(db: &Db, id_or_key: &str) -> AppResult<Customer> {
    let document = repository::find_customer_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
        })?;

    Ok(document.into_customer())
}
