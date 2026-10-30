use super::coll::coll;
use super::meta::META;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use mongodb::bson::doc;
use mongodb::options::ReturnDocument;

/// Allocates the next per-tenant sequence number (the pull cursor). Only the
/// single change-stream consumer calls this, so seq order equals commit order.
pub(crate) async fn allocate_seq(db: &Db) -> AppResult<i64> {
    let updated = coll(db, META)?
        .find_one_and_update(
            doc! { "kind": "meta" },
            doc! { "$inc": { "last_seq": 1_i64 } },
        )
        .upsert(true)
        .return_document(ReturnDocument::After)
        .await?;
    updated
        .and_then(|d| d.get_i64("last_seq").ok())
        .ok_or_else(|| AppError::internal("failed to allocate a sync sequence number"))
}
