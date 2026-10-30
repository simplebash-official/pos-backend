use super::repository_product_part1::build_product_where_clause;
use crate::clients::db::Db;
use crate::clients::sqlite::{SqlFilter, bson_to_iso, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::AppResult;
use crate::domain::inventory::Product;
use crate::modules::inventory::model::{CategoryDocument, ProductDocument, SubcategoryDocument};
use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;
use serde::Deserialize;
use sqlx::Row;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub(super) struct ProductWithLookups {
    #[serde(flatten)]
    pub(super) product: ProductDocument,
    #[serde(default)]
    pub(super) category_docs: Vec<CategoryDocument>,
    #[serde(default)]
    pub(super) subcategory_docs: Vec<SubcategoryDocument>,
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
            if sort.contains_key(MIN_SELLING_PRICE) {
                pipeline.extend(min_price_stages());
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

            // Case-insensitive collation so "apple" and "Apple" sort together.
            let mut cursor = collection
                .aggregate_with_collation(
                    pipeline,
                    mongodb::options::Collation::builder()
                        .locale("en")
                        .strength(mongodb::options::CollationStrength::Secondary)
                        .build(),
                )
                .await?;
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
            let SqlFilter {
                clause: where_clause,
                bindings,
            } = build_product_where_clause(&filter);

            let count_sql = format!("SELECT COUNT(*) FROM products p {}", where_clause);
            let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
            for b in &bindings {
                count_q = count_q.bind(b);
            }
            let total: i64 = count_q.fetch_one(pool).await?;

            let sort_order_clause = sqlite_order_by(&sort);

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

/// Same-module `$lookup` adding `min_selling_price_cents` (the lowest
/// selling price among the product's in-stock batches) so a `$sort` on it
/// works. Product keys are globally unique, so the join can't cross tenants.
pub(super) fn min_price_stages() -> Vec<Document> {
    vec![
        doc! { "$lookup": {
            "from": "stock_batches",
            "localField": "key",
            "foreignField": "product_key",
            "as": "_price_batches",
        } },
        doc! { "$addFields": { MIN_SELLING_PRICE: { "$min": { "$map": {
            "input": { "$filter": {
                "input": "$_price_batches",
                "cond": { "$and": [
                    { "$gt": ["$$this.remaining_quantity", 0] },
                    { "$not": ["$$this.deleted_at"] },
                ] },
            } },
            "in": "$$this.selling_price_cents",
        } } } } },
        doc! { "$project": { "_price_batches": 0 } },
    ]
}

/// Sort field standing for "a product's cheapest in-stock price". A product
/// stores no price (its batches do), so this is computed per row while
/// sorting: `min_price_stages` (Mongo) / `MIN_PRICE_SQL` (SQLite).
pub(crate) const MIN_SELLING_PRICE: &str = "min_selling_price_cents";

#[derive(Debug)]
pub(crate) struct OverviewAggregateResult {
    pub total_items: u64,
    pub low_stock_alerts: u64,
    pub category_counts: HashMap<String, u64>,
    pub subcategory_data: HashMap<String, (u64, Vec<ProductDocument>)>,
}

pub(super) fn products(db: &TenantDatabase) -> ScopedCollection<ProductDocument> {
    db.collection("products")
}

pub(super) fn row_to_product_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProductDocument> {
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

const MIN_PRICE_SQL: &str = "(SELECT MIN(b.selling_price_cents) FROM stock_batches b \
     WHERE b.product_key = p.key AND b.remaining_quantity > 0 AND b.deleted_at IS NULL)";

/// Builds a deterministic `ORDER BY` from a (whitelisted) Mongo-style sort
/// document. Every key in the document is honoured in order; text columns
/// compare case-insensitively and `_id` maps to the `id` column. Field names
/// only ever come from `sort_field_for`'s whitelist, never from client input.
pub(super) fn sqlite_order_by(sort: &Document) -> String {
    let mut parts: Vec<String> = sort
        .iter()
        .map(|(field, order)| {
            if field == MIN_SELLING_PRICE {
                let dir = if order.as_i32() == Some(-1) {
                    "DESC"
                } else {
                    "ASC"
                };
                return format!("{MIN_PRICE_SQL} {dir}");
            }
            let column = if field == "_id" { "id" } else { field.as_str() };
            let dir = if order.as_i32() == Some(-1) {
                "DESC"
            } else {
                "ASC"
            };
            let collate = if matches!(column, "name" | "sku") {
                " COLLATE NOCASE"
            } else {
                ""
            };
            format!("p.{column}{collate} {dir}")
        })
        .collect();
    if parts.is_empty() {
        parts.push("p.name COLLATE NOCASE ASC".to_string());
    }
    format!("ORDER BY {}", parts.join(", "))
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
