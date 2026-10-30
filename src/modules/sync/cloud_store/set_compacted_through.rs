use super::coll::coll;
use super::meta::META;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn set_compacted_through(db: &Db, seq: i64) -> AppResult<()> {
    coll(db, META)?
        .update_one(
            doc! { "kind": "meta" },
            doc! { "$max": { "compacted_through_seq": seq } },
        )
        .upsert(true)
        .await?;
    Ok(())
}
