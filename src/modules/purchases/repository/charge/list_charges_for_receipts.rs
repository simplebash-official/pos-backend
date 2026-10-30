use super::charge_from_row::charge_from_row;
use super::charges::charges;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::purchases::model::ReceiptChargeDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every charge of the given receipts, oldest first.
pub(crate) async fn list_charges_for_receipts(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<Vec<ReceiptChargeDocument>> {
    if receipt_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = charges(db)
                .find(doc! { "receipt_key": { "$in": receipt_keys } })
                .sort(doc! { "created_at": 1, "key": 1 })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders = vec!["?"; receipt_keys.len()].join(", ");
            let sql = format!(
                "SELECT * FROM receipt_charges WHERE receipt_key IN ({placeholders}) ORDER BY created_at ASC, key ASC"
            );
            let mut query = sqlx::query(&sql);
            for key in receipt_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(charge_from_row).collect()
        }
    }
}
