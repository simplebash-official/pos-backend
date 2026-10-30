use super::product_serials::product_serials;
use super::row_to_serial_doc::row_to_serial_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductSerialDocument;

use mongodb::bson::doc;

pub(crate) async fn find_serial_by_number(
    db: &Db,
    serial_number: &str,
) -> AppResult<Option<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => Ok(product_serials(db)
            .find_one(doc! { "serial_number": serial_number })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM product_serials WHERE serial_number = ?")
                .bind(serial_number)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_serial_doc(&r)).transpose()
        }
    }
}
