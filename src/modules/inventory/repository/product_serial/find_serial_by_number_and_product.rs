use super::product_serials::product_serials;
use super::row_to_serial_doc::row_to_serial_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductSerialDocument;

use mongodb::bson::doc;

/// Looks up a serial by number, additionally scoped to a specific product —
/// used when validating a sale/return references a serial that actually
/// belongs to the product line it's attached to.
pub(crate) async fn find_serial_by_number_and_product(
    db: &Db,
    serial_number: &str,
    product_key: &str,
) -> AppResult<Option<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => Ok(product_serials(db)
            .find_one(doc! { "serial_number": serial_number, "product_key": product_key })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM product_serials WHERE serial_number = ? AND product_key = ?",
            )
            .bind(serial_number)
            .bind(product_key)
            .fetch_optional(pool)
            .await?;

            row.map(|r| row_to_serial_doc(&r)).transpose()
        }
    }
}
