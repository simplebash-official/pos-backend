use super::import_batches::import_batches;
use super::map_sqlite_row_to_batch::map_sqlite_row_to_batch;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::imports::model::ImportBatchDocument;
use mongodb::bson::doc;

pub(crate) async fn find_batch_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<ImportBatchDocument>> {
    match db {
        Db::Mongo(db) => Ok(import_batches(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM import_batches WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.map(|r| map_sqlite_row_to_batch(&r)).transpose()
        }
    }
}
