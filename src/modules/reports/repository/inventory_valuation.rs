// MongoDB aggregation queries for stock asset and inventory valuation.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc},
};

use sqlx::Row;

use crate::{
    clients::db::Db,
    core::error::AppResult,
    domain::reports::{CategoryValuationEntry, InventoryValuationResponse},
};

fn products(db: &Database) -> Collection<Document> {
    db.collection("products")
}

fn categories_coll(db: &Database) -> Collection<Document> {
    db.collection("categories")
}

/// Runs a `$facet` pipeline across `products` to compute total stock valuation at cost vs retail,
/// low-stock counts, and per-category aggregates.
pub(crate) async fn aggregate_inventory_valuation(
    db: &Db,
) -> AppResult<InventoryValuationResponse> {
    let mut response = InventoryValuationResponse {
        total_products_count: 0,
        total_stock_units: 0,
        total_cost_valuation_cents: 0,
        total_retail_valuation_cents: 0,
        potential_gross_profit_cents: 0,
        potential_margin_percentage: 0.0,
        low_stock_products_count: 0,
        out_of_stock_products_count: 0,
        categories: Vec::new(),
    };

    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! { "$match": { "deleted_at": null } },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "product_count": { "$sum": 1 },
                                    "total_units": { "$sum": "$stock_quantity" },
                                    "cost_valuation": {
                                        "$sum": { "$multiply": ["$stock_quantity", "$cost_price_cents"] }
                                    },
                                    "retail_valuation": {
                                        "$sum": { "$multiply": ["$stock_quantity", "$selling_price_cents"] }
                                    },
                                }
                            }
                        ],
                        "low_stock": [
                            {
                                "$match": {
                                    "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
                                }
                            },
                            { "$group": { "_id": null, "count": { "$sum": 1 } } }
                        ],
                        "out_of_stock": [
                            {
                                "$match": { "stock_quantity": { "$lte": 0 } }
                            },
                            { "$group": { "_id": null, "count": { "$sum": 1 } } }
                        ],
                        "by_category": [
                            {
                                "$group": {
                                    "_id": "$category_key",
                                    "product_count": { "$sum": 1 },
                                    "total_units": { "$sum": "$stock_quantity" },
                                    "cost_val": {
                                        "$sum": { "$multiply": ["$stock_quantity", "$cost_price_cents"] }
                                    },
                                    "retail_val": {
                                        "$sum": { "$multiply": ["$stock_quantity", "$selling_price_cents"] }
                                    },
                                }
                            }
                        ]
                    }
                },
            ];

            let mut cursor = products(db).aggregate(pipeline).await?;

            if let Some(result) = cursor.try_next().await? {
                if let Ok(totals) = result.get_array("totals")
                    && let Some(first) = totals.first().and_then(|v| v.as_document())
                {
                    response.total_products_count = first
                        .get_i32("product_count")
                        .map(|c| c as u64)
                        .unwrap_or(0);
                    response.total_stock_units = first.get_i64("total_units").unwrap_or(0);
                    response.total_cost_valuation_cents =
                        first.get_i64("cost_valuation").unwrap_or(0);
                    response.total_retail_valuation_cents =
                        first.get_i64("retail_valuation").unwrap_or(0);
                    response.potential_gross_profit_cents =
                        response.total_retail_valuation_cents - response.total_cost_valuation_cents;

                    if response.total_retail_valuation_cents > 0 {
                        response.potential_margin_percentage =
                            ((response.potential_gross_profit_cents as f64
                                / response.total_retail_valuation_cents as f64)
                                * 100.0)
                                .round();
                    }
                }

                if let Ok(low_stock) = result.get_array("low_stock")
                    && let Some(first) = low_stock.first().and_then(|v| v.as_document())
                {
                    response.low_stock_products_count =
                        first.get_i32("count").map(|c| c as u64).unwrap_or(0);
                }

                if let Ok(out_of_stock) = result.get_array("out_of_stock")
                    && let Some(first) = out_of_stock.first().and_then(|v| v.as_document())
                {
                    response.out_of_stock_products_count =
                        first.get_i32("count").map(|c| c as u64).unwrap_or(0);
                }

                // Fetch category names map
                let mut cat_map = std::collections::HashMap::new();
                let mut cat_cursor = categories_coll(db).find(doc! {}).await?;
                while let Some(cdoc) = cat_cursor.try_next().await? {
                    if let Ok(k) = cdoc.get_str("key")
                        && let Ok(name) = cdoc.get_str("name")
                    {
                        cat_map.insert(k.to_string(), name.to_string());
                    }
                }

                if let Ok(by_cat) = result.get_array("by_category") {
                    for item in by_cat {
                        if let Some(doc) = item.as_document() {
                            let category_key = doc.get_str("_id").unwrap_or("").to_string();
                            let category_name = cat_map
                                .get(&category_key)
                                .cloned()
                                .unwrap_or_else(|| category_key.clone());
                            let product_count =
                                doc.get_i32("product_count").map(|c| c as u64).unwrap_or(0);
                            let total_units = doc.get_i64("total_units").unwrap_or(0);
                            let cost_valuation_cents = doc.get_i64("cost_val").unwrap_or(0);
                            let retail_valuation_cents = doc.get_i64("retail_val").unwrap_or(0);
                            let potential_profit_cents =
                                retail_valuation_cents - cost_valuation_cents;

                            response.categories.push(CategoryValuationEntry {
                                category_key,
                                category_name,
                                product_count,
                                total_units,
                                cost_valuation_cents,
                                retail_valuation_cents,
                                potential_profit_cents,
                            });
                        }
                    }
                }
            }
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"SELECT 
                    COUNT(*) as product_count,
                    COALESCE(SUM(stock_quantity), 0) as total_units,
                    COALESCE(SUM(stock_quantity * cost_price_cents), 0) as cost_val,
                    COALESCE(SUM(stock_quantity * selling_price_cents), 0) as retail_val,
                    COALESCE(SUM(CASE WHEN stock_quantity <= min_stock_threshold THEN 1 ELSE 0 END), 0) as low_stock_count,
                    COALESCE(SUM(CASE WHEN stock_quantity <= 0 THEN 1 ELSE 0 END), 0) as out_of_stock_count
                FROM products WHERE deleted_at IS NULL"#,
            )
            .fetch_one(pool)
            .await?;

            let total_products: i64 = row.get("product_count");
            let total_units: i64 = row.get("total_units");
            let cost_val: i64 = row.get("cost_val");
            let retail_val: i64 = row.get("retail_val");
            let low_stock_count: i64 = row.get("low_stock_count");
            let out_of_stock_count: i64 = row.get("out_of_stock_count");

            response.total_products_count = total_products as u64;
            response.total_stock_units = total_units;
            response.total_cost_valuation_cents = cost_val;
            response.total_retail_valuation_cents = retail_val;
            response.potential_gross_profit_cents = retail_val - cost_val;
            if retail_val > 0 {
                response.potential_margin_percentage =
                    (((retail_val - cost_val) as f64 / retail_val as f64) * 100.0).round();
            }
            response.low_stock_products_count = low_stock_count as u64;
            response.out_of_stock_products_count = out_of_stock_count as u64;

            let cat_rows = sqlx::query(
                r#"SELECT 
                    p.category_key,
                    COALESCE(c.name, p.category_key) as category_name,
                    COUNT(*) as product_count,
                    COALESCE(SUM(p.stock_quantity), 0) as total_units,
                    COALESCE(SUM(p.stock_quantity * p.cost_price_cents), 0) as cost_val,
                    COALESCE(SUM(p.stock_quantity * p.selling_price_cents), 0) as retail_val
                FROM products p
                LEFT JOIN categories c ON p.category_key = c.key
                WHERE p.deleted_at IS NULL
                GROUP BY p.category_key"#,
            )
            .fetch_all(pool)
            .await?;

            for r in cat_rows {
                let cost_c: i64 = r.get("cost_val");
                let ret_c: i64 = r.get("retail_val");
                let p_cnt: i64 = r.get("product_count");
                response.categories.push(CategoryValuationEntry {
                    category_key: r.get("category_key"),
                    category_name: r.get("category_name"),
                    product_count: p_cnt as u64,
                    total_units: r.get("total_units"),
                    cost_valuation_cents: cost_c,
                    retail_valuation_cents: ret_c,
                    potential_profit_cents: ret_c - cost_c,
                });
            }
        }
    }

    Ok(response)
}
