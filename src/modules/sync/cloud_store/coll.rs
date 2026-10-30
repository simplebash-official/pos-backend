use crate::clients::db::Db;
use crate::clients::tenant_db::ScopedCollection;
use crate::core::error::{AppError, AppResult};

use mongodb::bson::Document;

/// The tenant-scoped collection `name` (ambient tenant of the running request).
pub(crate) fn coll(db: &Db, name: &str) -> AppResult<ScopedCollection<Document>> {
    Ok(db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .collection::<Document>(name))
}
