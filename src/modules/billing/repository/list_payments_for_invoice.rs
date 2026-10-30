use super::payment_from_sqlite_row::payment_from_sqlite_row;
use super::payments::payments;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::model::PaymentDocument;

use futures_util::TryStreamExt;

use mongodb::bson::doc;

pub(crate) async fn list_payments_for_invoice(
    db: &Db,
    invoice_key: &str,
) -> AppResult<Vec<PaymentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payments(db)
                .find(doc! { "invoice_key": invoice_key })
                .sort(doc! { "recorded_at": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM payments WHERE invoice_key = ? AND deleted_at IS NULL ORDER BY recorded_at ASC",
            )
            .bind(invoice_key)
            .fetch_all(pool)
            .await?;

            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(payment_from_sqlite_row(&row)?);
            }
            Ok(items)
        }
    }
}
