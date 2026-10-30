use super::credit_from_row::credit_from_row;
use super::credits::credits;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::purchases::model::SupplierCreditDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every credit and voiding entry against the given receipts.
pub(crate) async fn list_credits_for_receipts(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<Vec<SupplierCreditDocument>> {
    if receipt_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = credits(db)
                .find(doc! { "receipt_key": { "$in": receipt_keys } })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders = vec!["?"; receipt_keys.len()].join(", ");
            let sql =
                format!("SELECT * FROM supplier_credits WHERE receipt_key IN ({placeholders})");
            let mut query = sqlx::query(&sql);
            for key in receipt_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(credit_from_row).collect()
        }
    }
}
