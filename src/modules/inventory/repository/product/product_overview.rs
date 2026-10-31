//! Inventory overview aggregation over the product collection.

use super::product_listing::min_price_stages;
use super::product_shared::{
    MIN_SELLING_PRICE, build_product_where_clause, products, row_to_product_doc, sqlite_order_by,
};
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;
use crate::core::error::{AppError, AppResult};
use crate::modules::inventory::model::ProductDocument;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use sqlx::Row;
use std::collections::HashMap;

/// The documents of an array field of a facet result; a missing or mistyped array is an error.
fn facet_rows<'a>(
    parent: &'a Document,
    field: &str,
) -> AppResult<impl Iterator<Item = &'a Document>> {
    let rows = parent
        .get_array(field)
        .map_err(|err| AppError::internal(format!("facet field `{field}` is missing: {err}")))?;
    let documents = rows
        .iter()
        .map(|row| {
            row.as_document().ok_or_else(|| {
                AppError::internal(format!("facet field `{field}` holds a non-document"))
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(documents.into_iter())
}

/// A non-negative count field of a facet row, stored as Int32 or Int64.
fn facet_count(row: &Document, field: &str) -> AppResult<u64> {
    let count = row
        .get_i64(field)
        .or_else(|_| row.get_i32(field).map(i64::from))
        .map_err(|err| AppError::internal(format!("facet field `{field}` is invalid: {err}")))?;
    u64::try_from(count)
        .map_err(|_| AppError::internal(format!("facet field `{field}` is negative")))
}

/// The count of a `$group` facet branch; an empty branch means nothing matched, so zero.
fn count_from_facet_branch(result: &Document, branch: &str) -> AppResult<u64> {
    let rows = result
        .get_array(branch)
        .map_err(|err| AppError::internal(format!("facet branch `{branch}` is missing: {err}")))?;
    let Some(first) = rows.first() else {
        return Ok(0);
    };
    let row = first.as_document().ok_or_else(|| {
        AppError::internal(format!("facet branch `{branch}` row is not a document"))
    })?;
    facet_count(row, "count")
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

            let total_items = count_from_facet_branch(&result, "total_items")?;
            let low_stock_alerts = count_from_facet_branch(&result, "low_stock_alerts")?;

            let mut category_counts = HashMap::new();
            for entry in facet_rows(&result, "by_category")? {
                let key = entry.get_str("_id").map_err(|err| {
                    AppError::internal(format!("facet row has no category key: {err}"))
                })?;
                category_counts.insert(key.to_string(), facet_count(entry, "count")?);
            }

            let mut subcategory_data = HashMap::new();
            for entry in facet_rows(&result, "by_subcategory")? {
                let key = entry.get_str("_id").map_err(|err| {
                    AppError::internal(format!("facet row has no subcategory key: {err}"))
                })?;
                let total_items = facet_count(entry, "total_items")?;
                let mut products = Vec::new();
                for product_doc in facet_rows(entry, "products")? {
                    products.push(bson::deserialize_from_document::<ProductDocument>(
                        product_doc.clone(),
                    )?);
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

#[derive(Debug)]
pub(crate) struct OverviewAggregateResult {
    pub total_items: u64,
    pub low_stock_alerts: u64,
    pub category_counts: HashMap<String, u64>,
    pub subcategory_data: HashMap<String, (u64, Vec<ProductDocument>)>,
}
