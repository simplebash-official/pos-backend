use super::user_from_sqlite_row::user_from_sqlite_row;
use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, doc};

pub(crate) async fn find_user_by_id(db: &Db, id: ObjectId) -> AppResult<Option<UserDocument>> {
    match db {
        // `deleted_at: null` matches live users: a user deleted through sync (or
        // the REST delete) is a tombstone on Mongo and must not be found again.
        Db::Mongo(db) => Ok(users(db)
            .find_one(doc! { "_id": id, "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let id_hex = id.to_hex();
            let row = sqlx::query("SELECT * FROM users WHERE id = $1")
                .bind(&id_hex)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(user_from_sqlite_row))
        }
    }
}
