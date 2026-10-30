use super::payment_from_sqlite_row::payment_from_sqlite_row;
use super::payments::payments;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::model::PaymentDocument;

use mongodb::bson::doc;

/// Looks up a single payment by its unique model key (`pay_...`) — used by
/// `service::credit_notes::void_credit_note` to read back the original
/// amount/method of a refund leg it's about to reverse.
pub(crate) async fn find_payment_by_key(db: &Db, key: &str) -> AppResult<Option<PaymentDocument>> {
    match db {
        Db::Mongo(db) => Ok(payments(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM payments WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| payment_from_sqlite_row(&row)).transpose()
        }
    }
}
