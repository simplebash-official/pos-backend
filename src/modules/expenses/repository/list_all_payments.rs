use super::payment_from_row::payment_from_row;
use super::payments::payments;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpensePaymentDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every payment, oldest first. The ledger is shop-sized, so the service groups them in memory.
pub(crate) async fn list_all_payments(db: &Db) -> AppResult<Vec<ExpensePaymentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payments(db)
                .find(doc! {})
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM expense_payments ORDER BY created_at ASC")
                .fetch_all(pool)
                .await?;
            rows.iter().map(payment_from_row).collect()
        }
    }
}
