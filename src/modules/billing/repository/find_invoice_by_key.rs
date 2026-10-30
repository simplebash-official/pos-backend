use super::invoice_from_sqlite_row::invoice_from_sqlite_row;
use super::invoices::invoices;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::model::InvoiceDocument;

use mongodb::bson::doc;

pub(crate) async fn find_invoice_by_key(db: &Db, key: &str) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM invoices WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}
