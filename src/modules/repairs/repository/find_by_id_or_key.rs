use super::find_by_id::find_by_id;
use super::find_by_key::find_by_key;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<RepairDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_by_key(db, id_or_key).await
}
