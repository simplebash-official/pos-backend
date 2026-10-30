use super::categories::categories;
use super::from_row::from_row;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::cash_categories::model::CashCategoryDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every category of the shop, hidden and merged ones included; the service filters.
pub(crate) async fn list_all(db: &Db) -> AppResult<Vec<CashCategoryDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = categories(db).find(doc! {}).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM cash_categories")
                .fetch_all(pool)
                .await?;
            rows.iter().map(from_row).collect()
        }
    }
}
