use super::from_row::from_row;
use super::payees::payees;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::payees::model::PayeeDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every stored payee.
pub(crate) async fn list_all(db: &Db) -> AppResult<Vec<PayeeDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payees(db).find(doc! {}).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM payees").fetch_all(pool).await?;
            rows.iter().map(from_row).collect()
        }
    }
}
