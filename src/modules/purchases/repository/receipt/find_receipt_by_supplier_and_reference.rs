use super::receipt_from_row::receipt_from_row;
use super::receipts::receipts;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseReceiptDocument;

use mongodb::bson::doc;

/// The supplier's earlier receipt carrying the same invoice number, if any (the duplicate check).
pub(crate) async fn find_receipt_by_supplier_and_reference(
    db: &Db,
    supplier_key: &str,
    reference_no: &str,
) -> AppResult<Option<PurchaseReceiptDocument>> {
    match db {
        Db::Mongo(db) => Ok(receipts(db)
            .find_one(doc! { "supplier_key": supplier_key, "reference_no": reference_no })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM purchase_receipts WHERE supplier_key = ? AND reference_no = ? LIMIT 1",
            )
            .bind(supplier_key)
            .bind(reference_no)
            .fetch_optional(pool)
            .await?;
            Ok(row.as_ref().map(receipt_from_row))
        }
    }
}
