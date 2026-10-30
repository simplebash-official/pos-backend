use super::find_link::find_link;
use super::supplier_products::supplier_products;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn update_link(
    db: &Db,
    supplier_key: &str,
    product_key: &str,
    set_doc: Document,
) -> AppResult<Option<SupplierProductLinkDocument>> {
    match db {
        Db::Mongo(db) => Ok(supplier_products(db)
            .find_one_and_update(
                doc! { "supplier_key": supplier_key, "product_key": product_key },
                doc! {
                    "$set": set_doc,
                    "$inc": { "version": 1 },
                    "$unset": { "deleted_at": "" },
                },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let existing = find_link(db, supplier_key, product_key).await?;
            let Some(mut link) = existing else {
                return Ok(None);
            };

            if set_doc.contains_key("cost_price_cents") {
                link.cost_price_cents = set_doc.get_i64("cost_price_cents").ok();
            }
            if set_doc.contains_key("notes") {
                link.notes = set_doc.get_str("notes").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("updated_by_device") {
                link.updated_by_device = set_doc
                    .get_str("updated_by_device")
                    .ok()
                    .map(|s| s.to_string());
            }
            link.deleted_at = None;
            link.version += 1;
            link.updated_at = BsonDateTime::now();

            let updated_at_iso = bson_to_iso(&link.updated_at);

            sqlx::query(
                r#"
                UPDATE supplier_products SET
                    cost_price_cents = ?, notes = ?, updated_by_device = ?,
                    version = ?, updated_at = ?, deleted_at = NULL
                WHERE supplier_key = ? AND product_key = ?
                "#,
            )
            .bind(link.cost_price_cents)
            .bind(&link.notes)
            .bind(&link.updated_by_device)
            .bind(link.version)
            .bind(&updated_at_iso)
            .bind(supplier_key)
            .bind(product_key)
            .execute(pool)
            .await?;

            Ok(Some(link))
        }
    }
}
