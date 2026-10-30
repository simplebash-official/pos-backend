use super::repairs::repairs;
use super::row_to_repair_doc::row_to_repair_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;

use mongodb::bson::doc;

pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<RepairDocument>> {
    match db {
        Db::Mongo(db) => Ok(repairs(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query("SELECT * FROM repairs WHERE key = ? AND deleted_at IS NULL")
                .bind(key)
                .fetch_optional(pool)
                .await?;

            row_opt.map(|row| row_to_repair_doc(&row)).transpose()
        }
    }
}
