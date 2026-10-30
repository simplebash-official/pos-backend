use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::users::Role;

use mongodb::bson::{Bson, doc};

/// Backs the single-Admin invariant in `service::create_user` — this is a
/// single-shop POS deployment, so there should never be more than one
/// Admin account.
pub(crate) async fn count_users_by_role(db: &Db, role: Role) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(users(db)
            .count_documents(doc! { "role": role.as_str(), "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = $1")
                .bind(role.as_str())
                .fetch_one(pool)
                .await?;
            Ok(row.0 as u64)
        }
    }
}
