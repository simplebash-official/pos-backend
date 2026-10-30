use super::supplier_product_link_sqlite_row::SupplierProductLinkSqliteRow;
use super::supplier_products::supplier_products;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;
use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};

pub(crate) async fn list_links_paginated(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<SupplierProductLinkDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let collection = supplier_products(db);
            let total = collection.count_documents(filter.clone()).await?;

            let mut cursor = collection
                .find(filter)
                .sort(doc! { "created_at": 1 })
                .skip(skip)
                .limit(limit)
                .await?;

            let mut items = Vec::new();
            while let Some(doc) = cursor.try_next().await? {
                items.push(doc);
            }
            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let supplier_key = filter.get_str("supplier_key").ok();
            let product_key = filter.get_str("product_key").ok();

            let (rows, total) = match (supplier_key, product_key) {
                (Some(sk), Some(pk)) => {
                    let total: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM supplier_products WHERE supplier_key = ? AND product_key = ? AND deleted_at IS NULL",
                    )
                    .bind(sk)
                    .bind(pk)
                    .fetch_one(pool)
                    .await?;

                    let rows = sqlx::query_as::<_, SupplierProductLinkSqliteRow>(
                        "SELECT * FROM supplier_products WHERE supplier_key = ? AND product_key = ? AND deleted_at IS NULL ORDER BY created_at ASC LIMIT ? OFFSET ?",
                    )
                    .bind(sk)
                    .bind(pk)
                    .bind(limit)
                    .bind(skip as i64)
                    .fetch_all(pool)
                    .await?;

                    (rows, total as u64)
                }
                (Some(sk), None) => {
                    let total: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM supplier_products WHERE supplier_key = ? AND deleted_at IS NULL",
                    )
                    .bind(sk)
                    .fetch_one(pool)
                    .await?;

                    let rows = sqlx::query_as::<_, SupplierProductLinkSqliteRow>(
                        "SELECT * FROM supplier_products WHERE supplier_key = ? AND deleted_at IS NULL ORDER BY created_at ASC LIMIT ? OFFSET ?",
                    )
                    .bind(sk)
                    .bind(limit)
                    .bind(skip as i64)
                    .fetch_all(pool)
                    .await?;

                    (rows, total as u64)
                }
                (None, Some(pk)) => {
                    let total: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM supplier_products WHERE product_key = ? AND deleted_at IS NULL",
                    )
                    .bind(pk)
                    .fetch_one(pool)
                    .await?;

                    let rows = sqlx::query_as::<_, SupplierProductLinkSqliteRow>(
                        "SELECT * FROM supplier_products WHERE product_key = ? AND deleted_at IS NULL ORDER BY created_at ASC LIMIT ? OFFSET ?",
                    )
                    .bind(pk)
                    .bind(limit)
                    .bind(skip as i64)
                    .fetch_all(pool)
                    .await?;

                    (rows, total as u64)
                }
                (None, None) => {
                    let total: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM supplier_products WHERE deleted_at IS NULL",
                    )
                    .fetch_one(pool)
                    .await?;

                    let rows = sqlx::query_as::<_, SupplierProductLinkSqliteRow>(
                        "SELECT * FROM supplier_products WHERE deleted_at IS NULL ORDER BY created_at ASC LIMIT ? OFFSET ?",
                    )
                    .bind(limit)
                    .bind(skip as i64)
                    .fetch_all(pool)
                    .await?;

                    (rows, total as u64)
                }
            };

            Ok((
                rows.into_iter()
                    .map(SupplierProductLinkSqliteRow::into_document)
                    .collect(),
                total,
            ))
        }
    }
}
