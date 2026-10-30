use super::find_invoice_by_id::find_invoice_by_id;
use super::invoice_from_sqlite_row::invoice_from_sqlite_row;
use super::invoices::invoices;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::model::InvoiceDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_invoice_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<InvoiceDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_invoice_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    match db {
        Db::Mongo(db) => Ok(invoices(db)
            .find_one(doc! {
                "$or": [
                    { "key": id_or_key },
                    { "invoice_number": id_or_key },
                ]
            })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query(
                "SELECT * FROM invoices WHERE (key = ? OR invoice_number = ?) AND deleted_at IS NULL",
            )
            .bind(id_or_key)
            .bind(id_or_key)
            .fetch_optional(pool)
            .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}
