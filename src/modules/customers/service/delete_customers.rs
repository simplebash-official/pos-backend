use super::delete_customer::delete_customer;
use crate::clients::db::Db;

use crate::core::error::AppResult;

pub async fn delete_customers(db: &Db, ids: Vec<String>) -> AppResult<u64> {
    crate::core::logging::domain::tracked("customers.bulk_deleted", async move {
        let mut deleted_count = 0u64;
        for id in ids {
            if delete_customer(db, &id, None, None).await.is_ok() {
                deleted_count += 1;
            }
        }
        Ok(deleted_count)
    })
    .await
}
