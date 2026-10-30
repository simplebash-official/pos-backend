use super::repository_product_part2::{
    MIN_SELLING_PRICE, OverviewAggregateResult, min_price_stages, products, row_to_product_doc,
    sqlite_order_by,
};
use crate::clients::db::Db;
use crate::clients::sqlite::{SqlFilter, bson_to_iso, generate_id_hex};
use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::inventory::model::ProductDocument;
use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;
use sqlx::Row;
use std::collections::HashMap;

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
            if sort.contains_key(MIN_SELLING_PRICE) {
                by_subcategory.extend(min_price_stages());
            }
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

            let SqlFilter {
                clause: where_clause,
                bindings,
            } = build_product_where_clause(&filter);

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

            let sort_order_clause = sqlite_order_by(&sort);

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

pub(super) fn build_product_where_clause(filter: &Document) -> SqlFilter {
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
                                if let Some(like) = like_pattern(pat) {
                                    or_parts.push("p.name LIKE ? ESCAPE '\\'".to_string());
                                    bindings.push(like);
                                }
                            } else if let Ok(pat) = or_doc.get_document("sku") {
                                if let Some(like) = like_pattern(pat) {
                                    or_parts.push("p.sku LIKE ? ESCAPE '\\'".to_string());
                                    bindings.push(like);
                                }
                            } else if let Ok(pat) = or_doc.get_document("barcode") {
                                if let Some(like) = like_pattern(pat) {
                                    or_parts.push("p.barcode LIKE ? ESCAPE '\\'".to_string());
                                    bindings.push(like);
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
    SqlFilter {
        clause: where_clause,
        bindings,
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

/// The SQLite `LIKE` pattern (`%text%`, with `%`, `_` and `\` escaped for
/// `ESCAPE '\'`) for a `{ "$regex": … }` search clause. `service::product`
/// sends a BSON regex built by `build_bson_regex`, whose text is regex-escaped,
/// so the escaping is undone first; a plain string is taken as it is.
pub(super) fn like_pattern(clause: &Document) -> Option<String> {
    let regex = match clause.get("$regex")? {
        Bson::RegularExpression(regex) => regex.pattern.to_string(),
        Bson::String(text) => text.clone(),
        _ => return None,
    };
    let mut text = String::with_capacity(regex.len() + 2);
    text.push('%');
    let mut chars = regex.chars();
    while let Some(c) = chars.next() {
        // `regex_escape` puts a backslash before each special character.
        let literal = match c {
            '\\' => chars.next().unwrap_or('\\'),
            other => other,
        };
        if matches!(literal, '%' | '_' | '\\') {
            text.push('\\');
        }
        text.push(literal);
    }
    text.push('%');
    Some(text)
}
