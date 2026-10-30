use super::charge_from_row::charge_from_row;
use super::charges::charges;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::purchase_receipts::BilledBy;
use crate::modules::purchases::model::ReceiptChargeDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every charge the supplier put on its bills, for the supplier balance.
pub(crate) async fn list_supplier_billed_charges(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<ReceiptChargeDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = charges(db)
                .find(
                    doc! { "supplier_key": supplier_key, "billed_by": BilledBy::Supplier.as_str() },
                )
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM receipt_charges WHERE supplier_key = ? AND billed_by = 'supplier'",
            )
            .bind(supplier_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(charge_from_row).collect()
        }
    }
}
