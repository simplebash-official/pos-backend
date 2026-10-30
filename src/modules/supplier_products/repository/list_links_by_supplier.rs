use super::supplier_product_link_sqlite_row::SupplierProductLinkSqliteRow;
use super::supplier_products::supplier_products;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;
use futures_util::TryStreamExt;

use mongodb::bson::doc;

pub(crate) async fn list_links_by_supplier(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<SupplierProductLinkDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = supplier_products(db)
                .find(doc! { "supplier_key": supplier_key })
                .sort(doc! { "created_at": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query_as::<_, SupplierProductLinkSqliteRow>(
                "SELECT * FROM supplier_products WHERE supplier_key = ? AND deleted_at IS NULL ORDER BY created_at ASC",
            )
            .bind(supplier_key)
            .fetch_all(pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(SupplierProductLinkSqliteRow::into_document)
                .collect())
        }
    }
}
