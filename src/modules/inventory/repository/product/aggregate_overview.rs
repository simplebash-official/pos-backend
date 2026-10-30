use super::build_product_where_clause::build_product_where_clause;
use super::min_price_stages::min_price_stages;
use super::min_selling_price::MIN_SELLING_PRICE;
use super::overview_aggregate_result::OverviewAggregateResult;
use super::products::products;
use super::row_to_product_doc::row_to_product_doc;
use super::sqlite_order_by::sqlite_order_by;
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;
use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};

use sqlx::Row;
use std::collections::HashMap;

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
