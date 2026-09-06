use serde::Deserialize;
use std::collections::HashMap;

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    domain::inventory::Product,
    modules::inventory::model::{CategoryDocument, ProductDocument, SubcategoryDocument},
};

fn products(db: &Database) -> Collection<ProductDocument> {
    db.collection("products")
}

fn row_to_product_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProductDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let barcode_source_str: Option<String> = row.get("barcode_source");
    let barcode_source = barcode_source_str.and_then(|s| s.parse().ok());
    let is_serialized_int: i64 = row.get("is_serialized");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(ProductDocument {
        id: object_id,
        key: row.get("key"),
        sku: row.get("sku"),
        barcode: row.get("barcode"),
        barcode_source,
        name: row.get("name"),
        category_key: row.get("category_key"),
        subcategory_key: row.get("subcategory_key"),
        cost_price_cents: row.get("cost_price_cents"),
        selling_price_cents: row.get("selling_price_cents"),
        stock_quantity: row.get("stock_quantity"),
        min_stock_threshold: row.get("min_stock_threshold"),
        is_serialized: is_serialized_int != 0,
        warranty_months: row.get("warranty_months"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}

fn build_product_where_clause(filter: &Document) -> (String, Vec<String>) {
    let mut conditions = vec!["p.deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    if let Ok(key_in) = filter.get_document("key")
        && let Ok(arr) = key_in.get_array("$in")
    {
        let placeholders: Vec<_> = arr.iter().map(|_| "?").collect();
        conditions.push(format!("p.key IN ({})", placeholders.join(",")));
        for val in arr {
            if let Some(s) = val.as_str() {
                bindings.push(s.to_string());
            }
        }
    }

    if let Ok(cat_key) = filter.get_str("category_key") {
        conditions.push("p.category_key = ?".to_string());
        bindings.push(cat_key.to_string());
    }

    if let Ok(subcat_key) = filter.get_str("subcategory_key") {
        conditions.push("p.subcategory_key = ?".to_string());
        bindings.push(subcat_key.to_string());
    }

    if let Ok(and_arr) = filter.get_array("$and") {
        for item in and_arr {
            if let Some(item_doc) = item.as_document() {
                if let Ok(cat_key) = item_doc.get_str("category_key") {
                    conditions.push("p.category_key = ?".to_string());
                    bindings.push(cat_key.to_string());
                }
                if let Ok(subcat_key) = item_doc.get_str("subcategory_key") {
                    conditions.push("p.subcategory_key = ?".to_string());
                    bindings.push(subcat_key.to_string());
                }
                if item_doc.contains_key("$expr") {
                    conditions.push("p.stock_quantity <= p.min_stock_threshold".to_string());
                }
                if let Ok(or_arr) = item_doc.get_array("$or") {
                    let mut or_parts = Vec::new();
                    for or_item in or_arr {
                        if let Some(or_doc) = or_item.as_document() {
                            if let Ok(pat) = or_doc.get_document("name") {
                                if let Ok(s) = pat.get_str("$regex") {
                                    or_parts.push("p.name LIKE ?".to_string());
                                    bindings.push(format!("%{}%", s));
                                }
                            } else if let Ok(pat) = or_doc.get_document("sku") {
                                if let Ok(s) = pat.get_str("$regex") {
                                    or_parts.push("p.sku LIKE ?".to_string());
                                    bindings.push(format!("%{}%", s));
                                }
                            } else if let Ok(pat) = or_doc.get_document("barcode") {
                                if let Ok(s) = pat.get_str("$regex") {
                                    or_parts.push("p.barcode LIKE ?".to_string());
                                    bindings.push(format!("%{}%", s));
                                }
                            } else if let Ok(cat_in) = or_doc.get_document("category_key") {
                                if let Ok(arr) = cat_in.get_array("$in") {
                                    let ph: Vec<_> = arr.iter().map(|_| "?").collect();
                                    or_parts.push(format!("p.category_key IN ({})", ph.join(",")));
                                    for v in arr {
                                        if let Some(s) = v.as_str() {
                                            bindings.push(s.to_string());
                                        }
                                    }
                                }
                            } else if let Ok(subcat_in) = or_doc.get_document("subcategory_key")
                                && let Ok(arr) = subcat_in.get_array("$in")
                            {
                                let ph: Vec<_> = arr.iter().map(|_| "?").collect();
                                or_parts.push(format!("p.subcategory_key IN ({})", ph.join(",")));
                                for v in arr {
                                    if let Some(s) = v.as_str() {
                                        bindings.push(s.to_string());
                                    }
                                }
                            }
                        }
                    }
                    if !or_parts.is_empty() {
                        conditions.push(format!("({})", or_parts.join(" OR ")));
                    }
                }
            }
        }
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    (where_clause, bindings)
}

pub(crate) async fn find_product_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM products WHERE id = ? AND deleted_at IS NULL")
                .bind(id.to_hex())
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
                    cost_price_cents, selling_price_cents, stock_quantity, min_stock_threshold,
                    is_serialized, warranty_months, version, created_at, updated_at, deleted_at,
                    updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            .bind(document.cost_price_cents)
            .bind(document.selling_price_cents)
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
                doc.barcode_source = barcode_source.parse().ok();
            }
            if let Ok(category_key) = set_doc.get_str("category_key") {
                doc.category_key = category_key.to_string();
            }
            if let Ok(subcategory_key) = set_doc.get_str("subcategory_key") {
                doc.subcategory_key = subcategory_key.to_string();
            }
            if let Ok(cost) = set_doc.get_i64("cost_price_cents") {
                doc.cost_price_cents = cost;
            }
            if let Ok(selling) = set_doc.get_i64("selling_price_cents") {
                doc.selling_price_cents = selling;
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
                    category_key = ?, subcategory_key = ?, cost_price_cents = ?,
                    selling_price_cents = ?, min_stock_threshold = ?, is_serialized = ?,
                    warranty_months = ?, version = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(&doc.name)
            .bind(&doc.sku)
            .bind(&doc.barcode)
            .bind(&barcode_source_str)
            .bind(&doc.category_key)
            .bind(&doc.subcategory_key)
            .bind(doc.cost_price_cents)
            .bind(doc.selling_price_cents)
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

pub(crate) async fn adjust_product_stock_pipeline(
    db: &Db,
    id: ObjectId,
    delta: i64,
    now: BsonDateTime,
) -> AppResult<Option<(i64, ProductDocument)>> {
    match db {
        Db::Mongo(db) => {
            let mut filter = doc! { "_id": id };
            if delta < 0 {
                filter.insert("stock_quantity", doc! { "$gte": -delta });
            }

            let pipeline_update = vec![doc! {
                "$set": {
                    "stock_quantity": { "$add": ["$stock_quantity", delta] },
                    "updated_at": now,
                    "version": { "$add": [{ "$ifNull": ["$version", 1] }, 1] },
                }
            }];

            let result = products(db)
                .find_one_and_update(filter, pipeline_update)
                .return_document(ReturnDocument::After)
                .await?;

            if let Some(updated) = result {
                let previous_stock = updated.stock_quantity - delta;
                Ok(Some((previous_stock, updated)))
            } else {
                Ok(None)
            }
        }
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let updated_at_iso = bson_to_iso(&now);

            // Atomic single-statement update with stock floor guard (stock_quantity + delta >= 0)
            // and RETURNING * to eliminate multi-statement transaction concurrency deadlocks.
            let row = sqlx::query(
                r#"
                UPDATE products
                SET stock_quantity = stock_quantity + ?,
                    updated_at = ?,
                    version = version + 1
                WHERE id = ?
                  AND deleted_at IS NULL
                  AND (stock_quantity + ? >= 0)
                RETURNING *
                "#,
            )
            .bind(delta)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .bind(delta)
            .fetch_optional(pool)
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

#[derive(Debug, Deserialize)]
struct ProductWithLookups {
    #[serde(flatten)]
    product: ProductDocument,
    #[serde(default)]
    category_docs: Vec<CategoryDocument>,
    #[serde(default)]
    subcategory_docs: Vec<SubcategoryDocument>,
}

pub(crate) async fn list_products_with_display_names(
    db: &Db,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<Product>, u64)> {
    match db {
        Db::Mongo(db) => {
            let collection = products(db);
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }

            let total = collection.count_documents(effective_filter.clone()).await?;

            let mut pipeline = Vec::new();
            if !effective_filter.is_empty() {
                pipeline.push(doc! { "$match": effective_filter });
            }
            if !sort.is_empty() {
                pipeline.push(doc! { "$sort": sort });
            }
            if skip > 0 {
                pipeline.push(doc! { "$skip": skip as i64 });
            }
            if limit > 0 {
                pipeline.push(doc! { "$limit": limit });
            }

            pipeline.push(doc! {
                "$lookup": {
                    "from": "categories",
                    "localField": "category_key",
                    "foreignField": "key",
                    "as": "category_docs"
                }
            });
            pipeline.push(doc! {
                "$lookup": {
                    "from": "subcategories",
                    "localField": "subcategory_key",
                    "foreignField": "key",
                    "as": "subcategory_docs"
                }
            });

            let mut cursor = collection.aggregate(pipeline).await?;
            let mut items = Vec::new();

            while let Some(doc) = cursor.try_next().await? {
                let lookup_item = ProductWithLookups::deserialize(bson::Deserializer::new(
                    bson::Bson::Document(doc),
                ))?;
                let category_name = lookup_item
                    .category_docs
                    .first()
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| lookup_item.product.category_key.clone());

                let subcategory_name = lookup_item
                    .subcategory_docs
                    .first()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| lookup_item.product.subcategory_key.clone());

                items.push(
                    lookup_item
                        .product
                        .into_product(category_name, subcategory_name),
                );
            }

            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let (where_clause, bindings) = build_product_where_clause(&filter);

            let count_sql = format!("SELECT COUNT(*) FROM products p {}", where_clause);
            let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
            for b in &bindings {
                count_q = count_q.bind(b);
            }
            let total: i64 = count_q.fetch_one(pool).await?;

            let mut sort_order_clause = "ORDER BY p.name ASC".to_string();
            if let Some((field, order)) = sort.iter().next() {
                let dir = if order.as_i32() == Some(-1) {
                    "DESC"
                } else {
                    "ASC"
                };
                sort_order_clause = format!("ORDER BY p.{} {}", field, dir);
            }

            let select_sql = format!(
                r#"
                SELECT p.*, c.name AS category_name, s.name AS subcategory_name
                FROM products p
                LEFT JOIN categories c ON p.category_key = c.key
                LEFT JOIN subcategories s ON p.subcategory_key = s.key
                {}
                {}
                LIMIT ? OFFSET ?
                "#,
                where_clause, sort_order_clause
            );

            let mut select_q = sqlx::query(&select_sql);
            for b in &bindings {
                select_q = select_q.bind(b);
            }
            select_q = select_q.bind(limit).bind(skip as i64);

            let rows = select_q.fetch_all(pool).await?;
            let mut items = Vec::new();

            for r in &rows {
                let doc = row_to_product_doc(r)?;
                let cat_name: Option<String> = r.get("category_name");
                let sub_name: Option<String> = r.get("subcategory_name");
                let category_name = cat_name.unwrap_or_else(|| doc.category_key.clone());
                let subcategory_name = sub_name.unwrap_or_else(|| doc.subcategory_key.clone());

                items.push(doc.into_product(category_name, subcategory_name));
            }

            Ok((items, total as u64))
        }
    }
}

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

fn count_from_facet_branch(result: &Document, branch: &str) -> u64 {
    result
        .get_array(branch)
        .ok()
        .and_then(|arr| arr.first())
        .and_then(|val| val.as_document())
        .and_then(|doc| {
            doc.get_i32("count")
                .ok()
                .map(|c| c as u64)
                .or_else(|| doc.get_i64("count").ok().map(|c| c as u64))
        })
        .unwrap_or(0)
}

pub(crate) struct OverviewAggregateResult {
    pub total_items: u64,
    pub low_stock_alerts: u64,
    pub category_counts: HashMap<String, u64>,
    pub subcategory_data: HashMap<String, (u64, Vec<ProductDocument>)>,
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

pub(crate) async fn aggregate_overview(
    db: &Db,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: i64,
) -> AppResult<OverviewAggregateResult> {
    match db {
        Db::Mongo(db) => {
            let mut by_category = Vec::new();
            let mut by_subcategory = Vec::new();
            if !filter.is_empty() {
                by_category.push(doc! { "$match": filter.clone() });
                by_subcategory.push(doc! { "$match": filter });
            }
            by_category.push(doc! {
                "$group": { "_id": "$category_key", "count": { "$sum": 1 } }
            });
            by_subcategory.push(doc! { "$sort": sort });
            by_subcategory.push(doc! {
                "$group": {
                    "_id": "$subcategory_key",
                    "total_items": { "$sum": 1 },
                    "products": { "$push": "$$ROOT" }
                }
            });
            by_subcategory.push(doc! {
                "$project": {
                    "total_items": 1,
                    "products": { "$slice": ["$products", skip as i64, limit] }
                }
            });

            let pipeline = vec![doc! {
                "$facet": {
                    "total_items": [ { "$count": "count" } ],
                    "low_stock_alerts": [
                        { "$match": { "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] } } },
                        { "$count": "count" }
                    ],
                    "by_category": by_category,
                    "by_subcategory": by_subcategory,
                }
            }];

            let mut cursor = products(db).aggregate(pipeline).await?;
            let Some(result) = cursor.try_next().await? else {
                return Ok(OverviewAggregateResult {
                    total_items: 0,
                    low_stock_alerts: 0,
                    category_counts: HashMap::new(),
                    subcategory_data: HashMap::new(),
                });
            };

            let total_items = count_from_facet_branch(&result, "total_items");
            let low_stock_alerts = count_from_facet_branch(&result, "low_stock_alerts");

            let mut category_counts = HashMap::new();
            for entry in result.get_array("by_category").ok().into_iter().flatten() {
                if let Some(entry) = entry.as_document()
                    && let Ok(key) = entry.get_str("_id")
                {
                    let count = entry
                        .get_i32("count")
                        .ok()
                        .map(|c| c as u64)
                        .or_else(|| entry.get_i64("count").ok().map(|c| c as u64))
                        .unwrap_or(0);
                    category_counts.insert(key.to_string(), count);
                }
            }

            let mut subcategory_data = HashMap::new();
            for entry in result
                .get_array("by_subcategory")
                .ok()
                .into_iter()
                .flatten()
            {
                let Some(entry) = entry.as_document() else {
                    continue;
                };
                let Ok(key) = entry.get_str("_id") else {
                    continue;
                };
                let total_items = entry
                    .get_i32("total_items")
                    .ok()
                    .map(|c| c as u64)
                    .or_else(|| entry.get_i64("total_items").ok().map(|c| c as u64))
                    .unwrap_or(0);
                let mut products = Vec::new();
                for product_doc in entry.get_array("products").ok().into_iter().flatten() {
                    if let Some(product_doc) = product_doc.as_document() {
                        products.push(bson::deserialize_from_document::<ProductDocument>(
                            product_doc.clone(),
                        )?);
                    }
                }
                subcategory_data.insert(key.to_string(), (total_items, products));
            }

            Ok(OverviewAggregateResult {
                total_items,
                low_stock_alerts,
                category_counts,
                subcategory_data,
            })
        }
        Db::Sqlite(pool) => {
            let total_items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
                .fetch_one(pool)
                .await?;
            let low_stock_alerts: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM products WHERE stock_quantity <= min_stock_threshold",
            )
            .fetch_one(pool)
            .await?;

            let (where_clause, bindings) = build_product_where_clause(&filter);

            let cat_sql = format!(
                "SELECT p.category_key, COUNT(*) as count FROM products p {} GROUP BY p.category_key",
                where_clause
            );
            let mut cat_q = sqlx::query(&cat_sql);
            for b in &bindings {
                cat_q = cat_q.bind(b);
            }
            let cat_rows = cat_q.fetch_all(pool).await?;

            let mut category_counts = HashMap::new();
            for r in cat_rows {
                let cat_key: String = r.get("category_key");
                let count: i64 = r.get("count");
                category_counts.insert(cat_key, count as u64);
            }

            let subcat_sql = format!(
                "SELECT p.subcategory_key, COUNT(*) as count FROM products p {} GROUP BY p.subcategory_key",
                where_clause
            );
            let mut subcat_q = sqlx::query(&subcat_sql);
            for b in &bindings {
                subcat_q = subcat_q.bind(b);
            }
            let subcat_rows = subcat_q.fetch_all(pool).await?;

            let mut sort_order_clause = "ORDER BY p.name ASC".to_string();
            if let Some((field, order)) = sort.iter().next() {
                let dir = if order.as_i32() == Some(-1) {
                    "DESC"
                } else {
                    "ASC"
                };
                sort_order_clause = format!("ORDER BY p.{} {}", field, dir);
            }

            let mut subcategory_data = HashMap::new();
            for r in subcat_rows {
                let sub_key: String = r.get("subcategory_key");
                let count: i64 = r.get("count");

                let mut sub_filter_bindings = bindings.clone();
                let sub_where = if where_clause.is_empty() || where_clause == "WHERE " {
                    "WHERE p.subcategory_key = ?".to_string()
                } else {
                    format!("{} AND p.subcategory_key = ?", where_clause)
                };
                sub_filter_bindings.push(sub_key.clone());

                let sub_p_sql = format!(
                    "SELECT p.* FROM products p {} {} LIMIT ? OFFSET ?",
                    sub_where, sort_order_clause
                );
                let mut sub_p_q = sqlx::query(&sub_p_sql);
                for b in &sub_filter_bindings {
                    sub_p_q = sub_p_q.bind(b);
                }
                sub_p_q = sub_p_q.bind(limit).bind(skip as i64);

                let p_rows = sub_p_q.fetch_all(pool).await?;
                let mut products = Vec::new();
                for pr in p_rows {
                    products.push(row_to_product_doc(&pr)?);
                }

                subcategory_data.insert(sub_key, (count as u64, products));
            }

            Ok(OverviewAggregateResult {
                total_items: total_items as u64,
                low_stock_alerts: low_stock_alerts as u64,
                category_counts,
                subcategory_data,
            })
        }
    }
}
