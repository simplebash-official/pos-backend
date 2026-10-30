use super::credit_from_row::credit_from_row;
use super::credits::credits;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::purchases::model::SupplierCreditDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every credit and voiding entry of one supplier, against a receipt or not.
pub(crate) async fn list_credits_for_supplier(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<SupplierCreditDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = credits(db)
                .find(doc! { "supplier_key": supplier_key })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM supplier_credits WHERE supplier_key = ?")
                .bind(supplier_key)
                .fetch_all(pool)
                .await?;
            rows.iter().map(credit_from_row).collect()
        }
    }
}
