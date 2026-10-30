use super::products::products;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_product(
    db: &Db,
    mut document: ProductDocument,
) -> AppResult<ProductDocument> {
    match db {
        Db::Mongo(db) => {
            let result = products(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let deleted_at_iso = document.deleted_at.as_ref().map(bson_to_iso);
            let barcode_source_str = document.barcode_source.map(|s| s.as_str().to_string());

            sqlx::query(
                r#"
                INSERT INTO products (
                    key, id, sku, barcode, barcode_source, name, category_key, subcategory_key,
                    stock_quantity, min_stock_threshold, is_serialized, warranty_months, version,
                    created_at, updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.sku)
            .bind(&document.barcode)
            .bind(&barcode_source_str)
            .bind(&document.name)
            .bind(&document.category_key)
            .bind(&document.subcategory_key)
            .bind(document.stock_quantity)
            .bind(document.min_stock_threshold)
            .bind(if document.is_serialized { 1 } else { 0 })
            .bind(document.warranty_months)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
