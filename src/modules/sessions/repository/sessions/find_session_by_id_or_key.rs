use super::session_from_sqlite_row::session_from_sqlite_row;
use super::sessions_col::sessions_col;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::model::RegisterSessionDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub async fn find_session_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Db::Mongo(mongo_db) = db
        && let Some(doc) = sessions_col(mongo_db)
            .find_one(doc! { "_id": object_id })
            .await?
    {
        return Ok(Some(doc));
    }
    match db {
        Db::Mongo(mongo_db) => {
            let doc = sessions_col(mongo_db)
                .find_one(doc! {
                    "$or": [
                        { "key": id_or_key },
                        { "session_number": id_or_key },
                    ]
                })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM register_sessions WHERE (key = ? OR session_number = ? OR id = ?) AND deleted_at IS NULL"
            )
            .bind(id_or_key)
            .bind(id_or_key)
            .bind(id_or_key)
            .fetch_optional(pool)
            .await?;

            row.map(|r| session_from_sqlite_row(&r)).transpose()
        }
    }
}
