use super::products::products;
use super::row_to_product_doc::row_to_product_doc;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn delete_product(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let mut set_doc = doc! {
                "deleted_at": now,
                "updated_at": now,
            };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(products(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": set_doc,
                        "$inc": { "version": 1 }
                    },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
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
            let now = BsonDateTime::now();
            doc.deleted_at = Some(now);
            doc.updated_at = now;
            doc.version += 1;
            doc.updated_by_device = device_id.clone();

            let now_iso = bson_to_iso(&now);

            sqlx::query(
                "UPDATE products SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ?",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(&device_id)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(doc))
        }
    }
}
