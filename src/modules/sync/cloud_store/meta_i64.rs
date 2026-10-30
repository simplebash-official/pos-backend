use super::coll::coll;
use super::meta::META;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(super) async fn meta_i64(db: &Db, field: &str) -> AppResult<i64> {
    Ok(coll(db, META)?
        .find_one(doc! { "kind": "meta" })
        .await?
        .and_then(|d| d.get_i64(field).ok())
        .unwrap_or(0))
}
