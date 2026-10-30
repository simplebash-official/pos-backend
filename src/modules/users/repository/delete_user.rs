use super::find_user_by_id::find_user_by_id;
use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn delete_user(db: &Db, id: ObjectId) -> AppResult<Option<UserDocument>> {
    match db {
        // A tombstone, not a removal: the cloud change stream only sees updates,
        // so this is what carries the delete to every device. Returns the user as
        // it was before deletion, like the SQLite arm.
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            Ok(users(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": Bson::Null },
                    doc! { "$set": { "deleted_at": now, "updated_at": now } },
                )
                .return_document(ReturnDocument::Before)
                .await?)
        }
        Db::Sqlite(pool) => {
            let user = find_user_by_id(db, id).await?;
            if user.is_some() {
                let id_hex = id.to_hex();
                sqlx::query("DELETE FROM users WHERE id = $1")
                    .bind(&id_hex)
                    .execute(pool)
                    .await?;
            }
            Ok(user)
        }
    }
}
