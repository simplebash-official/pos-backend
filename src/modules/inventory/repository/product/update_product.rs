use super::products::products;
use super::row_to_product_doc::row_to_product_doc;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn update_product(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let row = sqlx::query("SELECT * FROM products WHERE id = ? AND deleted_at IS NULL")
                .bind(&id_str)
                .fetch_optional(pool)
                .await?;

            let Some(row) = row else {
                return Ok(None);
            };

            let mut doc = row_to_product_doc(&row)?;

            if let Ok(name) = set_doc.get_str("name") {
                doc.name = name.to_string();
            }
            if let Ok(sku) = set_doc.get_str("sku") {
                doc.sku = sku.to_string();
            }
            if let Ok(barcode) = set_doc.get_str("barcode") {
                doc.barcode = Some(barcode.to_string());
            }
            if let Ok(barcode_source) = set_doc.get_str("barcode_source") {
                doc.barcode_source = barcode_source.parse().ok();
            }
            if let Ok(category_key) = set_doc.get_str("category_key") {
                doc.category_key = category_key.to_string();
            }
            if let Ok(subcategory_key) = set_doc.get_str("subcategory_key") {
                doc.subcategory_key = subcategory_key.to_string();
            }
            if let Ok(min_stock) = set_doc.get_i64("min_stock_threshold") {
                doc.min_stock_threshold = min_stock;
            }
            if let Ok(is_ser) = set_doc.get_bool("is_serialized") {
                doc.is_serialized = is_ser;
            }
            if let Ok(wm) = set_doc.get_i64("warranty_months") {
                doc.warranty_months = Some(wm);
            }
            if let Ok(dt) = set_doc.get_datetime("updated_at") {
                doc.updated_at = *dt;
            } else {
                doc.updated_at = BsonDateTime::now();
            }
            doc.version += 1;

            let updated_at_iso = bson_to_iso(&doc.updated_at);
            let barcode_source_str = doc.barcode_source.map(|s| s.as_str().to_string());

            sqlx::query(
                r#"
                UPDATE products SET
                    name = ?, sku = ?, barcode = ?, barcode_source = ?,
                    category_key = ?, subcategory_key = ?, min_stock_threshold = ?,
                    is_serialized = ?, warranty_months = ?, version = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(&doc.name)
            .bind(&doc.sku)
            .bind(&doc.barcode)
            .bind(&barcode_source_str)
            .bind(&doc.category_key)
            .bind(&doc.subcategory_key)
            .bind(doc.min_stock_threshold)
            .bind(if doc.is_serialized { 1 } else { 0 })
            .bind(doc.warranty_months)
            .bind(doc.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(doc))
        }
    }
}
