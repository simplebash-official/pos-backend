use serde::Deserialize;
use std::collections::HashMap;

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{
    core::error::AppResult,
    domain::inventory::Product,
    modules::inventory::model::{CategoryDocument, ProductDocument, SubcategoryDocument},
};

fn products(db: &Database) -> Collection<ProductDocument> {
    db.collection("products")
}

pub(crate) async fn find_product_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db)
        .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
        .await?)
}

/// Used for the create-time uniqueness check (`service::product::create_product`) —
/// SKUs must be unique across all products.
pub(crate) async fn find_product_by_sku(
    db: &Database,
    sku: &str,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db)
        .find_one(doc! { "sku": sku, "deleted_at": { "$exists": false } })
        .await?)
}

/// Used for the manual-barcode-collision check and the generated-barcode
/// defensive fallback check in `service::product::create_product`.
pub(crate) async fn find_product_by_barcode(
    db: &Database,
    barcode: &str,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db)
        .find_one(doc! { "barcode": barcode, "deleted_at": { "$exists": false } })
        .await?)
}

/// Looked up by `key` rather than `_id` — the entry point for other modules
/// (e.g. `supplier_products`/`purchases`) that only hold a product's
/// immutable `key`, never its `ObjectId`.
pub(crate) async fn find_product_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db)
        .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
        .await?)
}

/// Fetches every product matching one of `ids` in a single query — used by
/// `service::product::delete_products` to read each product's `key` (for
/// the `supplier_products` cascade) before the documents are deleted.
pub(crate) async fn find_products_by_ids(
    db: &Database,
    ids: &[ObjectId],
) -> AppResult<Vec<ProductDocument>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut cursor = products(db)
        .find(doc! { "_id": { "$in": ids }, "deleted_at": { "$exists": false } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Inserts `document` and backfills its `id` from the driver-generated
/// `_id`, since the caller builds the document with `id: None` (Mongo
/// assigns the `ObjectId` on insert, not before).
pub(crate) async fn insert_product(
    db: &Database,
    mut document: ProductDocument,
) -> AppResult<ProductDocument> {
    let result = products(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// `set_doc` is built by the caller (`service::product::update_product`),
/// which decides which fields actually change — this just applies it.
pub(crate) async fn update_product(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_product(
    db: &Database,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<ProductDocument>> {
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

/// Batch delete for `DELETE /products`. Guards the empty-list case
/// explicitly — an empty `$in` array is valid Mongo but there's no reason
/// to round-trip to the database for a no-op delete.
pub(crate) async fn delete_products(db: &Database, ids: Vec<ObjectId>) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
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

/// `$expr`/`$lte` compares two fields of the *same* document
/// (`stock_quantity` vs `min_stock_threshold`) — a plain field-to-value
/// filter can't express that, so this needs the aggregation-style `$expr`
/// operator even though it's a simple `find`, not an aggregation pipeline.
pub(crate) async fn find_low_stock_products(db: &Database) -> AppResult<Vec<ProductDocument>> {
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

/// Atomically updates a product's stock using a MongoDB 4.2+ pipeline update (`vec!["$set": ...]`).
/// When `delta < 0`, includes `stock_quantity >= -delta` in the query filter to atomically prevent negative stock.
pub(crate) async fn adjust_product_stock_pipeline(
    db: &Database,
    id: ObjectId,
    delta: i64,
    now: BsonDateTime,
) -> AppResult<Option<(i64, ProductDocument)>> {
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

#[derive(Debug, Deserialize)]
struct ProductWithLookups {
    #[serde(flatten)]
    product: ProductDocument,
    #[serde(default)]
    category_docs: Vec<CategoryDocument>,
    #[serde(default)]
    subcategory_docs: Vec<SubcategoryDocument>,
}

/// Runs a MongoDB `$lookup` aggregation pipeline over `products` to join display names
/// from `categories` and `subcategories` in a single query round-trip.
pub(crate) async fn list_products_with_display_names(
    db: &Database,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<Product>, u64)> {
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
        let lookup_item =
            ProductWithLookups::deserialize(bson::Deserializer::new(bson::Bson::Document(doc)))?;
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

/// Backs the "category still in use" 409 guard on category delete
/// (`service::category::delete_category`) — a category can't be removed
/// while products still reference it by `category_key`.
pub(crate) async fn count_products_in_category(
    db: &Database,
    category_key: &str,
) -> AppResult<u64> {
    Ok(products(db)
        .count_documents(doc! { "category_key": category_key, "deleted_at": { "$exists": false } })
        .await?)
}

/// Same guard as `count_products_in_category`, scoped to a single
/// subcategory — used before removing a subcategory from a category.
pub(crate) async fn count_products_in_subcategory(
    db: &Database,
    subcategory_key: &str,
) -> AppResult<u64> {
    Ok(products(db)
        .count_documents(
            doc! { "subcategory_key": subcategory_key, "deleted_at": { "$exists": false } },
        )
        .await?)
}

/// Reads a `$count`-stage result (`[{ "count": N }]`, possibly empty) out of
/// one branch of a `$facet` result document.
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

/// Everything the Inventory & Stock overview page (`service::overview::get_inventory_overview`)
/// needs from the `products` collection, computed in one aggregation round-trip
/// (see `aggregate_overview`).
pub(crate) struct OverviewAggregateResult {
    /// Total product count across the whole collection, ignoring the current filter —
    /// the dashboard-header metric, not a filtered count.
    pub total_items: u64,
    /// Count of products at or below their `min_stock_threshold`, also unfiltered.
    pub low_stock_alerts: u64,
    /// Matching product count per `category_key`, respecting the current filter.
    pub category_counts: HashMap<String, u64>,
    /// Per `subcategory_key`: matching product count plus its already-paginated
    /// (sorted, skipped, limited) page of `ProductDocument`s.
    pub subcategory_data: HashMap<String, (u64, Vec<ProductDocument>)>,
}

/// Counts non-deleted products, and how many of those are at/below their
/// `min_stock_threshold`, for `GET /inventory/stats`. Unlike
/// `aggregate_overview`'s `total_items`/`low_stock_alerts` branches (which
/// intentionally mirror the dashboard's existing unfiltered-count behavior),
/// this excludes soft-deleted rows — the correct count for a fresh stats
/// endpoint with no prior behavior to preserve.
pub(crate) async fn count_stats(db: &Database) -> AppResult<(u64, u64)> {
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

/// Runs a single Mongo `$facet` aggregation pipeline over `products` to compute
/// everything the overview endpoint needs in one database round-trip: unfiltered
/// dashboard metrics (`total_items`/`low_stock_alerts`), filtered per-category
/// product counts, and filtered+sorted+paginated per-subcategory product pages —
/// replacing what would otherwise be a separate query per subcategory.
pub(crate) async fn aggregate_overview(
    db: &Database,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: i64,
) -> AppResult<OverviewAggregateResult> {
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
