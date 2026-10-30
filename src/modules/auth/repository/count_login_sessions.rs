use super::login_sessions::login_sessions;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use mongodb::bson::Document;

pub(crate) async fn count_login_sessions(db: &Db, filter: Document) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(login_sessions(db).count_documents(filter).await?),
        Db::Sqlite(pool) => {
            let user_key = filter.get_str("user_key").ok();
            let count: i64 = match user_key {
                Some(uk) => {
                    sqlx::query_scalar("SELECT COUNT(*) FROM login_sessions WHERE user_key = ?")
                        .bind(uk)
                        .fetch_one(pool)
                        .await?
                }
                None => {
                    sqlx::query_scalar("SELECT COUNT(*) FROM login_sessions")
                        .fetch_one(pool)
                        .await?
                }
            };
            Ok(count as u64)
        }
    }
}
