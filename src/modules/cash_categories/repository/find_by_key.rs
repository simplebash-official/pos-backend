use super::categories::categories;
use super::from_row::from_row;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::cash_categories::model::CashCategoryDocument;
use mongodb::bson::doc;

/// One category by its key.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<CashCategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM cash_categories WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}
