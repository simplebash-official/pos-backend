use super::invoice_from_sqlite_row::invoice_from_sqlite_row;
use super::invoices::invoices;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::model::InvoiceDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_invoice_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db).find_one(doc! { "_id": id }).await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query("SELECT * FROM invoices WHERE id = ? AND deleted_at IS NULL")
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}
