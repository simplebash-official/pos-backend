use super::payments::payments;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;
use sqlx::Row;

/// Used by `service::sale::cancel_invoice`'s guard (D8): cancellation is
/// blocked once more than the original sale-time payment(s) exist.
pub(crate) async fn count_payments_for_invoice(db: &Db, invoice_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(payments(db)
            .count_documents(doc! { "invoice_key": invoice_key })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT COUNT(*) as cnt FROM payments WHERE invoice_key = ? AND deleted_at IS NULL",
            )
            .bind(invoice_key)
            .fetch_one(pool)
            .await?;
            let cnt: i64 = row.get("cnt");
            Ok(cnt as u64)
        }
    }
}
