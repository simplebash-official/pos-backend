//! Product inserts, updates, deletes and the atomic stock adjustment.

use super::product_shared::{products, row_to_product_doc};
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::clients::uow::{Backend, Unit};
use crate::core::error::{AppError, AppResult};
use crate::modules::inventory::model::ProductDocument;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn adjust_product_stock_pipeline(
    unit: impl Into<Unit<'_>>,
    id: ObjectId,
    delta: i64,
) -> AppResult<Option<(i64, ProductDocument)>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let mut filter = doc! { "_id": id };
            if delta < 0 {
                filter.insert("stock_quantity", doc! { "$gte": -delta });
            }

            // Only the stock moves: `stock_quantity` is derived from the
            // stock-movement ledger, so this is not an edit of the product.
            // Bumping `updated_at`/`version` (or the writer) here would let a
            // sale win last-writer-wins over a newer price or name edit made
            // on another device.
            let pipeline_update = vec![doc! {
                "$set": {
                    "stock_quantity": { "$add": ["$stock_quantity", delta] },
                }
            }];

            let result = crate::in_session!(
                products(db)
                    .preserve_origin()
                    .find_one_and_update(filter, pipeline_update)
                    .return_document(ReturnDocument::After),
                session
            )?;

            if let Some(updated) = result {
                let previous_stock = updated.stock_quantity - delta;
                Ok(Some((previous_stock, updated)))
            } else {
                Ok(None)
            }
        }
        Backend::Sqlite(mut conn) => {
            let id_str = id.to_hex();

            // Atomic single-statement update with stock floor guard (stock_quantity + delta >= 0)
            // and RETURNING * to eliminate multi-statement transaction concurrency deadlocks.
            // Only `stock_quantity` changes (see the Mongo arm): the sync
            // capture trigger ignores derived-only writes, so a sale does not
            // re-send the product row.
            let row = sqlx::query(
                r#"
                UPDATE products
                SET stock_quantity = stock_quantity + ?
                WHERE id = ?
                  AND deleted_at IS NULL
                  AND (stock_quantity + ? >= 0)
                RETURNING *
                "#,
            )
            .bind(delta)
            .bind(&id_str)
            .bind(delta)
            .fetch_optional(&mut *conn)
            .await?;

            if let Some(row) = row {
                let updated = row_to_product_doc(&row)?;
                let previous_stock = updated.stock_quantity - delta;
                Ok(Some((previous_stock, updated)))
            } else {
                Ok(None)
            }
        }
    }
}

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

pub(crate) async fn delete_products(db: &Db, ids: Vec<ObjectId>) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let result = products(db)
                .update_many(
                    doc! { "_id": { "$in": ids }, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": { "deleted_at": now, "updated_at": now },
                        "$inc": { "version": 1 }
                    },
                )
                .await?;
            Ok(result.modified_count)
        }
        Db::Sqlite(pool) => {
            let now_iso = bson_to_iso(&BsonDateTime::now());
            let placeholders: Vec<_> = ids.iter().map(|_| "?").collect();
            let query_str = format!(
                "UPDATE products SET deleted_at = ?, updated_at = ?, version = version + 1 WHERE id IN ({}) AND deleted_at IS NULL",
                placeholders.join(",")
            );
            let mut q = sqlx::query(&query_str).bind(&now_iso).bind(&now_iso);
            for id in ids {
                q = q.bind(id.to_hex());
            }
            let res = q.execute(pool).await?;
            Ok(res.rows_affected())
        }
    }
}

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
                doc.barcode_source = Some(barcode_source.parse().map_err(|_| {
                    AppError::internal(format!("barcode source `{barcode_source}` is unknown"))
                })?);
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
