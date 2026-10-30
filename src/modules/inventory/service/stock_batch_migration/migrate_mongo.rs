use super::super::stock_batch::backfill_line_allocations;
use super::mode::Mode;
use super::report::Report;
use super::single_shop::SINGLE_SHOP;
use super::summary::Summary;
use crate::core::constants::prefixes::opening_batch_key;
use crate::core::tenancy::TENANT_FIELD;
use futures_util::{StreamExt, TryStreamExt, stream};
use mongodb::bson::{Bson, Document, doc};
use mongodb::{Database, bson};
use std::collections::HashMap;

fn tenant_of(document: &Document) -> String {
    match document.get(TENANT_FIELD) {
        Some(Bson::String(tenant)) => tenant.clone(),
        Some(other) => other.to_string(),
        None => SINGLE_SHOP.to_string(),
    }
}

fn i64_of(document: &Document, field: &str) -> i64 {
    match document.get(field) {
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Int32(v)) => i64::from(*v),
        Some(Bson::Double(v)) => *v as i64,
        _ => 0,
    }
}

/// How many products / documents are migrated at once. Each costs a few round
/// trips, which over a remote cluster dominate the run time.
const CONCURRENCY: usize = 16;

/// Runs the migration over every tenant in `db` (see the module banner):
/// products first, then the sale lines that point at them, then the price
/// fields come off — each phase concurrent, every step idempotent. With
/// `Mode::DryRun` nothing is written and the report says what would be.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> mongodb::error::Result<Report> {
    let products: Vec<Document> = db
        .collection::<Document>("products")
        .find(doc! { "cost_price_cents": { "$exists": true } })
        .await?
        .try_collect()
        .await?;
    let costs: HashMap<String, i64> = products
        .iter()
        .filter_map(|p| {
            Some((
                p.get_str("key").ok()?.to_string(),
                i64_of(p, "cost_price_cents"),
            ))
        })
        .collect();

    let mut report = Report::default();
    let per_product: Vec<(String, Summary)> = stream::iter(products.iter())
        .map(|product| async move {
            Ok::<_, mongodb::error::Error>((
                tenant_of(product),
                migrate_product(db, product, mode).await?,
            ))
        })
        .buffer_unordered(CONCURRENCY)
        .try_collect()
        .await?;
    for (tenant, summary) in &per_product {
        report.shop(tenant.clone()).add(summary);
    }

    let cost_of = |key: &str| costs.get(key).copied();
    for (tenant, changed, unallocated) in
        backfill_lines(db, "invoices", &["items"], &cost_of, mode).await?
    {
        let shop = report.shop(tenant);
        shop.invoices += changed;
        shop.lines_without_cost += unallocated;
    }
    for (tenant, changed, unallocated) in backfill_lines(
        db,
        "credit_notes",
        &["returned_items", "exchange_items"],
        &cost_of,
        mode,
    )
    .await?
    {
        let shop = report.shop(tenant);
        shop.credit_notes += changed;
        shop.lines_without_cost += unallocated;
    }

    let migratable: Vec<&Document> = products
        .iter()
        .filter(|p| p.get_object_id("_id").is_ok() && p.get_str("key").is_ok())
        .collect();
    if mode == Mode::Apply {
        stream::iter(
            migratable
                .iter()
                .map(|p| p.get_object_id("_id").expect("filtered above")),
        )
        .map(|id| async move {
            db.collection::<Document>("products")
                .update_one(
                    doc! { "_id": id },
                    doc! { "$unset": { "cost_price_cents": "", "selling_price_cents": "" } },
                )
                .await
                .map(|_| ())
        })
        .buffer_unordered(CONCURRENCY)
        .try_collect::<Vec<()>>()
        .await?;
    }
    for product in migratable {
        report.shop(tenant_of(product)).products += 1;
    }
    Ok(report)
}

/// Creates one product's opening batch and points its movements, serials and
/// purchases at it — or, in a dry run, counts what that would touch.
async fn migrate_product(
    db: &Database,
    product: &Document,
    mode: Mode,
) -> mongodb::error::Result<Summary> {
    let mut summary = Summary::default();
    let (Ok(id), Ok(key)) = (product.get_object_id("_id"), product.get_str("key")) else {
        summary.skipped_products = 1;
        return Ok(summary);
    };
    if i64_of(product, "selling_price_cents") <= 0 {
        summary.zero_price_products = 1;
    }
    let batch_key = opening_batch_key(key);
    let movements_filter = doc! {
        "product_id": id,
        "batch_key": { "$exists": false },
        "quantity_delta": { "$ne": 0 },
    };
    let serials_filter = doc! { "product_key": key, "batch_key": { "$exists": false } };
    let purchases_filter = doc! { "product_key": key, "selling_price_cents": { "$exists": false } };

    if mode == Mode::DryRun {
        let exists = db
            .collection::<Document>("stock_batches")
            .find_one(doc! { "key": &batch_key })
            .await?
            .is_some();
        summary.batches = u64::from(!exists);
        summary.movements = db
            .collection::<Document>("stock_movements")
            .count_documents(movements_filter)
            .await?;
        summary.serials = db
            .collection::<Document>("product_serials")
            .count_documents(serials_filter)
            .await?;
        summary.purchases = db
            .collection::<Document>("purchases")
            .count_documents(purchases_filter)
            .await?;
        return Ok(summary);
    }

    let created_at = product
        .get_datetime("created_at")
        .copied()
        .unwrap_or_else(|_| bson::DateTime::now());
    let stock = i64_of(product, "stock_quantity");
    let mut batch = doc! {
        "_id": id,
        "key": &batch_key,
        "product_key": key,
        "source": "opening",
        "received_quantity": stock,
        "remaining_quantity": stock,
        "unit_cost_cents": i64_of(product, "cost_price_cents"),
        "selling_price_cents": i64_of(product, "selling_price_cents"),
        "received_at": created_at,
        "version": 1_i64,
        "created_at": created_at,
        "updated_at": created_at,
    };
    if let Ok(deleted_at) = product.get_datetime("deleted_at") {
        batch.insert("deleted_at", *deleted_at);
    }
    if let Some(tenant) = product.get(TENANT_FIELD) {
        batch.insert(TENANT_FIELD, tenant.clone());
    }
    let inserted = db
        .collection::<Document>("stock_batches")
        .update_one(doc! { "key": &batch_key }, doc! { "$setOnInsert": batch })
        .upsert(true)
        .await?;
    summary.batches = u64::from(inserted.upserted_id.is_some());

    summary.movements = db
        .collection::<Document>("stock_movements")
        .update_many(
            movements_filter,
            doc! { "$set": { "batch_key": &batch_key } },
        )
        .await?
        .modified_count;
    summary.serials = db
        .collection::<Document>("product_serials")
        .update_many(serials_filter, doc! { "$set": { "batch_key": &batch_key } })
        .await?
        .modified_count;
    summary.purchases = db
        .collection::<Document>("purchases")
        .update_many(
            purchases_filter,
            doc! { "$set": { "selling_price_cents": i64_of(product, "selling_price_cents") } },
        )
        .await?
        .modified_count;
    Ok(summary)
}

/// Product lines in `lines` (camelCase JSON) still without a batch allocation.
fn unallocated_lines(lines: &serde_json::Value) -> u64 {
    lines
        .as_array()
        .into_iter()
        .flatten()
        .filter(|line| {
            line.get("productKey").is_some()
                && line.get("quantity").and_then(|q| q.as_i64()).unwrap_or(0) > 0
                && !line
                    .get("batchAllocations")
                    .and_then(|a| a.as_array())
                    .is_some_and(|a| !a.is_empty())
        })
        .count() as u64
}

/// Rewrites each document's line arrays in `fields` through
/// `backfill_line_allocations` (not in a dry run); returns, per document's
/// shop, whether it changed and how many product lines stay unallocated.
/// Only documents with a product line still lacking allocations are read.
async fn backfill_lines(
    db: &Database,
    collection: &str,
    fields: &[&str],
    cost_of: &(impl Fn(&str) -> Option<i64> + Sync),
    mode: Mode,
) -> mongodb::error::Result<Vec<(String, u64, u64)>> {
    let collection = db.collection::<Document>(collection);
    let pending: Vec<Document> = fields
        .iter()
        .map(|field| {
            doc! { *field: { "$elemMatch": {
                "productKey": { "$exists": true },
                "batchAllocations": { "$exists": false },
            } } }
        })
        .collect();
    let documents: Vec<Document> = collection
        .find(doc! { "$or": pending })
        .await?
        .try_collect()
        .await?;
    let collection = &collection;
    stream::iter(documents)
        .map(|document| async move {
            let tenant = tenant_of(&document);
            let Ok(id) = document.get_object_id("_id") else {
                return Ok((tenant, 0_u64, 0_u64));
            };
            let mut set = Document::new();
            let mut unallocated = 0;
            for field in fields {
                let Some(lines) = document.get(*field) else {
                    continue;
                };
                let Ok(mut json) = serde_json::to_value(lines) else {
                    continue;
                };
                if backfill_line_allocations(&mut json, cost_of)
                    && let Ok(rewritten) = bson::serialize_to_bson(&json)
                {
                    set.insert(*field, rewritten);
                }
                unallocated += unallocated_lines(&json);
            }
            if set.is_empty() {
                return Ok((tenant, 0, unallocated));
            }
            if mode == Mode::Apply {
                collection
                    .update_one(doc! { "_id": id }, doc! { "$set": set })
                    .await?;
            }
            Ok((tenant, 1, unallocated))
        })
        .buffer_unordered(CONCURRENCY)
        .try_collect()
        .await
}
