use super::user_from_sqlite_row::user_from_sqlite_row;
use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;

use mongodb::bson::{Bson, doc};

/// Looked up by normalized (lowercase) username — the entry point both
/// `create_user`'s uniqueness check and `verify_credentials`'s login lookup
/// use. Scoped to the current shop by the tenant-aware collection.
pub(crate) async fn find_user_by_username(
    db: &Db,
    username: &str,
) -> AppResult<Option<UserDocument>> {
    match db {
        Db::Mongo(db) => Ok(users(db)
            .find_one(doc! { "username": username, "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM users WHERE LOWER(username) = LOWER($1)")
                .bind(username)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(user_from_sqlite_row))
        }
    }
}
