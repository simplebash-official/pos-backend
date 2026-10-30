use super::payment_from_row::payment_from_row;
use super::payments::payments;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpensePaymentDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// The payments of one expense, oldest first.
pub(crate) async fn list_payments_for(
    db: &Db,
    expense_key: &str,
) -> AppResult<Vec<ExpensePaymentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payments(db)
                .find(doc! { "expense_key": expense_key })
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM expense_payments WHERE expense_key = ? ORDER BY created_at ASC",
            )
            .bind(expense_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(payment_from_row).collect()
        }
    }
}
