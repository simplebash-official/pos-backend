use super::find_link::find_link;
use super::record_link_deletes::record_link_deletes;
use super::supplier_products::supplier_products;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;

use mongodb::bson::doc;

pub(crate) async fn delete_link(
    db: &Db,
    supplier_key: &str,
    product_key: &str,
) -> AppResult<Option<SupplierProductLinkDocument>> {
    match db {
        Db::Mongo(mongo) => {
            let deleted = supplier_products(mongo)
                .find_one_and_delete(
                    doc! { "supplier_key": supplier_key, "product_key": product_key },
                )
                .await?;
            if let Some(link) = &deleted {
                record_link_deletes(db, vec![link.key.clone()]).await?;
            }
            Ok(deleted)
        }
        Db::Sqlite(pool) => {
            let existing = find_link(db, supplier_key, product_key).await?;
            if existing.is_some() {
                sqlx::query(
                    "DELETE FROM supplier_products WHERE supplier_key = ? AND product_key = ?",
                )
                .bind(supplier_key)
                .bind(product_key)
                .execute(pool)
                .await?;
            }
            Ok(existing)
        }
    }
}
