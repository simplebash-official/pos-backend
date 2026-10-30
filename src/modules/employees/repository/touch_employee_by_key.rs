use super::employees::employees;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;
use crate::core::error::AppResult;
use mongodb::bson::{DateTime as BsonDateTime, doc};

pub(crate) async fn touch_employee_by_key(db: &Db, key: &str) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            employees(db)
                .update_one(
                    doc! { "key": key, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": { "updated_at": BsonDateTime::now() },
                        "$inc": { "version": 1 }
                    },
                )
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let now = now_utc_iso();
            sqlx::query(
                "UPDATE employees SET updated_at = ?, version = version + 1 WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(now)
            .bind(key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
