use super::find_customer_by_id::find_customer_by_id;
use super::find_customer_by_key::find_customer_by_key;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_customer_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<CustomerDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_customer_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_customer_by_key(db, id_or_key).await
}
