//! Single and bulk product lookups, and the counts taken from them.

use super::product_shared::{products, row_to_product_doc};
use crate::clients::db::Db;
use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::inventory::model::ProductDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn count_products_in_category(db: &Db, category_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .count_documents(
                doc! { "category_key": category_key, "deleted_at": { "$exists": false } },
            )
            .await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM products WHERE category_key = ? AND deleted_at IS NULL",
            )
            .bind(category_key)
            .fetch_one(pool)
            .await?;
            Ok(count as u64)
        }
    }
}

pub(crate) async fn count_products_in_subcategory(
    db: &Db,
    subcategory_key: &str,
) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .count_documents(
                doc! { "subcategory_key": subcategory_key, "deleted_at": { "$exists": false } },
            )
            .await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM products WHERE subcategory_key = ? AND deleted_at IS NULL",
            )
            .bind(subcategory_key)
            .fetch_one(pool)
            .await?;
            Ok(count as u64)
        }
    }
}

pub(crate) async fn count_stats(db: &Db) -> AppResult<(u64, u64)> {
    match db {
        Db::Mongo(db) => {
            let total_items = products(db)
                .count_documents(doc! { "deleted_at": { "$exists": false } })
                .await?;
            let low_stock_alerts = products(db)
                .count_documents(doc! {
                    "deleted_at": { "$exists": false },
                    "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
                })
                .await?;
            Ok((total_items, low_stock_alerts))
        }
        Db::Sqlite(pool) => {
            let total_items: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE deleted_at IS NULL")
                    .fetch_one(pool)
                    .await?;

            let low_stock_alerts: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM products WHERE deleted_at IS NULL AND stock_quantity <= min_stock_threshold",
            )
            .fetch_one(pool)
            .await?;

            Ok((total_items as u64, low_stock_alerts as u64))
        }
    }
}

pub(crate) async fn find_low_stock_products(db: &Db) -> AppResult<Vec<ProductDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = products(db)
                .find(doc! {
                    "deleted_at": { "$exists": false },
                    "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
                })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM products WHERE deleted_at IS NULL AND stock_quantity <= min_stock_threshold",
            )
            .fetch_all(pool)
            .await?;

            rows.iter().map(row_to_product_doc).collect()
        }
    }
}

pub(crate) async fn find_product_by_barcode(
    db: &Db,
    barcode: &str,
) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .find_one(doc! { "barcode": barcode, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row =
                sqlx::query("SELECT * FROM products WHERE barcode = ? AND deleted_at IS NULL")
                    .bind(barcode)
                    .fetch_optional(pool)
                    .await?;

            row.map(|r| row_to_product_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_product_by_barcode_excluding(
    db: &Db,
    barcode: &str,
    exclude_id: ObjectId,
) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .find_one(doc! {
                "barcode": barcode,
                "_id": { "$ne": exclude_id },
                "deleted_at": { "$exists": false },
            })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM products WHERE barcode = ? AND id != ? AND deleted_at IS NULL",
            )
            .bind(barcode)
            .bind(exclude_id.to_hex())
            .fetch_optional(pool)
            .await?;

            row.map(|r| row_to_product_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_product_by_id(
    unit: impl Into<Unit<'_>>,
    id: ObjectId,
) -> AppResult<Option<ProductDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => Ok(crate::in_session!(
            products(db).find_one(doc! { "_id": id, "deleted_at": { "$exists": false } }),
            session
        )?),
        Backend::Sqlite(mut conn) => {
            let row = sqlx::query("SELECT * FROM products WHERE id = ? AND deleted_at IS NULL")
                .bind(id.to_hex())
                .fetch_optional(&mut *conn)
                .await?;

            row.map(|r| row_to_product_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_product_by_key(db: &Db, key: &str) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM products WHERE key = ? AND deleted_at IS NULL")
                .bind(key)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_product_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_product_by_sku(db: &Db, sku: &str) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .find_one(doc! { "sku": sku, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM products WHERE sku = ? AND deleted_at IS NULL")
                .bind(sku)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_product_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_products_by_ids(
    db: &Db,
    ids: &[ObjectId],
) -> AppResult<Vec<ProductDocument>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = products(db)
                .find(doc! { "_id": { "$in": ids }, "deleted_at": { "$exists": false } })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders: Vec<_> = ids.iter().map(|_| "?").collect();
            let query_str = format!(
                "SELECT * FROM products WHERE id IN ({}) AND deleted_at IS NULL",
                placeholders.join(",")
            );
            let mut q = sqlx::query(&query_str);
            for id in ids {
                q = q.bind(id.to_hex());
            }
            let rows = q.fetch_all(pool).await?;
            rows.iter().map(row_to_product_doc).collect()
        }
    }
}
