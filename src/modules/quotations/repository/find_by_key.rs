use super::quotations::quotations;
use super::row_to_doc::row_to_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::quotations::model::QuotationDocument;

use mongodb::bson::doc;

pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => Ok(quotations(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM quotations WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.map(|r| row_to_doc(&r)).transpose()
        }
    }
}
