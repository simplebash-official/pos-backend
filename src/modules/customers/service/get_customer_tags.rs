use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::customers::CustomerTagsResponse;

use crate::modules::customers::repository;

pub async fn get_customer_tags(db: &Db) -> AppResult<CustomerTagsResponse> {
    let mut tags = repository::distinct_tags(db).await?;
    tags.sort();
    tags.dedup();
    Ok(CustomerTagsResponse { tags })
}
