//! The paged product list with category and subcategory display names.

use super::product_shared::{
    MIN_SELLING_PRICE, build_product_where_clause, products, row_to_product_doc, sqlite_order_by,
};
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;
use crate::core::error::AppResult;
use crate::domain::inventory::Product;
use crate::modules::inventory::model::{CategoryDocument, ProductDocument, SubcategoryDocument};
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use serde::Deserialize;
use sqlx::Row;

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
