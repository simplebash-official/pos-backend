// Dual-engine (Mongo / SQLite) access for the `supplier_products` collection.
// Functions here return `Option`/`Vec`/counts straight from the driver.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    modules::supplier_products::model::SupplierProductLinkDocument,
};

#[derive(sqlx::FromRow)]
struct SupplierProductLinkSqliteRow {
    id: String,
    key: String,
    supplier_key: String,
    product_key: String,
    cost_price_cents: Option<i64>,
    notes: Option<String>,
    version: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    updated_by_device: Option<String>,
}

impl SupplierProductLinkSqliteRow {
    fn into_document(self) -> SupplierProductLinkDocument {
        SupplierProductLinkDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            supplier_key: self.supplier_key,
            product_key: self.product_key,
            cost_price_cents: self.cost_price_cents,
            notes: self.notes,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}

fn supplier_products(db: &TenantDatabase) -> ScopedCollection<SupplierProductLinkDocument> {
    db.collection("supplier_products")
}

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

#[allow(dead_code)]
pub(crate) async fn list_links_by_product(
    db: &Db,
    product_key: &str,
) -> AppResult<Vec<SupplierProductLinkDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = supplier_products(db)
                .find(doc! { "product_key": product_key })
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
                "SELECT * FROM supplier_products WHERE product_key = ? AND deleted_at IS NULL ORDER BY created_at ASC",
            )
            .bind(product_key)
            .fetch_all(pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(SupplierProductLinkSqliteRow::into_document)
                .collect())
        }
    }
}

pub(crate) async fn insert_link(
    db: &Db,
    mut document: SupplierProductLinkDocument,
) -> AppResult<SupplierProductLinkDocument> {
    match db {
        Db::Mongo(db) => {
            let result = supplier_products(db).insert_one(&document).await?;
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
            let created_at = bson_to_iso(&document.created_at);
            let updated_at = bson_to_iso(&document.updated_at);
            let deleted_at = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO supplier_products (
                    key, id, supplier_key, product_key, cost_price_cents, notes,
                    version, created_at, updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.supplier_key)
            .bind(&document.product_key)
            .bind(document.cost_price_cents)
            .bind(&document.notes)
            .bind(document.version)
            .bind(&created_at)
            .bind(&updated_at)
            .bind(&deleted_at)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

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

pub(crate) async fn delete_link(
    db: &Db,
    supplier_key: &str,
    product_key: &str,
) -> AppResult<Option<SupplierProductLinkDocument>> {
    match db {
        Db::Mongo(db) => Ok(supplier_products(db)
            .find_one_and_delete(doc! { "supplier_key": supplier_key, "product_key": product_key })
            .await?),
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

pub(crate) async fn delete_links_by_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(supplier_products(db)
            .delete_many(doc! { "supplier_key": supplier_key })
            .await?
            .deleted_count),
        Db::Sqlite(pool) => {
            let res = sqlx::query("DELETE FROM supplier_products WHERE supplier_key = ?")
                .bind(supplier_key)
                .execute(pool)
                .await?;
            Ok(res.rows_affected())
        }
    }
}

pub(crate) async fn delete_links_by_product(db: &Db, product_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(supplier_products(db)
            .delete_many(doc! { "product_key": product_key })
            .await?
            .deleted_count),
        Db::Sqlite(pool) => {
            let res = sqlx::query("DELETE FROM supplier_products WHERE product_key = ?")
                .bind(product_key)
                .execute(pool)
                .await?;
            Ok(res.rows_affected())
        }
    }
}

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
