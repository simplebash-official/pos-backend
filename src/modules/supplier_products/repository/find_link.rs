use super::supplier_product_link_sqlite_row::SupplierProductLinkSqliteRow;
use super::supplier_products::supplier_products;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;

use mongodb::bson::doc;

pub(crate) async fn find_link(
    db: &Db,
    supplier_key: &str,
    product_key: &str,
) -> AppResult<Option<SupplierProductLinkDocument>> {
    match db {
        Db::Mongo(db) => Ok(supplier_products(db)
            .find_one(doc! { "supplier_key": supplier_key, "product_key": product_key })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query_as::<_, SupplierProductLinkSqliteRow>(
                "SELECT * FROM supplier_products WHERE supplier_key = ? AND product_key = ?",
            )
            .bind(supplier_key)
            .bind(product_key)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(SupplierProductLinkSqliteRow::into_document))
        }
    }
}
