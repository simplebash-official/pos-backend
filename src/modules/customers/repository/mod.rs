// Mongo access for the `customers` collection only. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`/
// counts straight from the driver and leave the "not found" -> `AppError`
// translation to `service`. Visibility is `pub(crate)` so `service` can call
// in, but the `mod repository;` declaration in `customers/mod.rs` is
// private, so none of this is reachable from outside the `customers` module
// tree.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, modules::customers::model::CustomerDocument};

fn customers(db: &Database) -> Collection<CustomerDocument> {
    db.collection("customers")
}

pub(crate) async fn find_customer_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<CustomerDocument>> {
    Ok(customers(db)
        .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
        .await?)
}

pub(crate) async fn find_customer_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<CustomerDocument>> {
    Ok(customers(db)
        .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
        .await?)
}

pub(crate) async fn find_customer_by_id_or_key(
    db: &Database,
    id_or_key: &str,
) -> AppResult<Option<CustomerDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_customer_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_customer_by_key(db, id_or_key).await
}

#[allow(dead_code)]
pub(crate) async fn find_customers_by_keys(
    db: &Database,
    keys: &[String],
) -> AppResult<Vec<CustomerDocument>> {
    let mut cursor = customers(db)
        .find(doc! { "key": { "$in": keys }, "deleted_at": { "$exists": false } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

pub(crate) async fn insert_customer(
    db: &Database,
    mut document: CustomerDocument,
) -> AppResult<CustomerDocument> {
    let result = customers(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

pub(crate) async fn update_customer(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<CustomerDocument>> {
    Ok(customers(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_customer(
    db: &Database,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<CustomerDocument>> {
    let now = BsonDateTime::now();
    let mut set_doc = doc! {
        "deleted_at": now,
        "updated_at": now,
    };
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }
    Ok(customers(db)
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

pub(crate) async fn list_customers(
    db: &Database,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<CustomerDocument>, u64)> {
    let mut effective_filter = filter;
    if !effective_filter.contains_key("deleted_at") {
        effective_filter.insert("deleted_at", doc! { "$exists": false });
    }

    let total = customers(db)
        .count_documents(effective_filter.clone())
        .await?;

    let mut cursor = customers(db)
        .find(effective_filter)
        .sort(sort)
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok((items, total))
}

pub(crate) async fn distinct_tags(db: &Database) -> AppResult<Vec<String>> {
    let values = customers(db)
        .distinct("tags", doc! { "deleted_at": { "$exists": false } })
        .await?;

    Ok(values
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect())
}

/// Atomically adjusts customer total purchases and outstanding balance via `$inc`.
/// Used by billing/invoices and payments modules.
#[allow(dead_code)]
pub(crate) async fn adjust_customer_financials(
    db: &Database,
    key: &str,
    purchases_delta: i64,
    balance_delta: i64,
) -> AppResult<Option<CustomerDocument>> {
    let now = BsonDateTime::now();
    Ok(customers(db)
        .find_one_and_update(
            doc! { "key": key, "deleted_at": { "$exists": false } },
            doc! {
                "$inc": {
                    "total_purchases_cents": purchases_delta,
                    "outstanding_balance_cents": balance_delta,
                    "version": 1,
                },
                "$set": {
                    "updated_at": now,
                }
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

/// Everything `service::get_customer_stats` needs from `customers`, computed
/// in one aggregation round-trip.
pub(crate) struct StatsAggregateResult {
    /// Count of non-deleted customers.
    pub total_customers: u64,
    /// Sum of `outstanding_balance_cents` across non-deleted customers with
    /// a positive balance.
    pub total_balance_due_cents: i64,
    /// Count of those same customers.
    pub active_debtors_count: u64,
}

/// Extracts a scalar `i64` field pushed by a `$group`/`$count` facet branch,
/// out of that branch's array-of-one-document shape, empty array if the
/// branch matched nothing.
fn i64_from_facet_branch(result: &Document, branch: &str, field: &str) -> i64 {
    result
        .get_array(branch)
        .ok()
        .and_then(|arr| arr.first())
        .and_then(|val| val.as_document())
        .and_then(|doc| {
            doc.get_i64(field)
                .ok()
                .or_else(|| doc.get_i32(field).ok().map(i64::from))
        })
        .unwrap_or(0)
}

/// Runs a single `$facet` aggregation over `customers` to compute the
/// Customers screen's KPI cards in one database round-trip: total count, and
/// the debtors sum + count.
pub(crate) async fn aggregate_stats(db: &Database) -> AppResult<StatsAggregateResult> {
    let not_deleted = doc! { "deleted_at": { "$exists": false } };
    let pipeline = vec![doc! {
        "$facet": {
            "total": [
                { "$match": not_deleted.clone() },
                { "$count": "count" }
            ],
            "debtors": [
                { "$match": { "$and": [not_deleted, { "outstanding_balance_cents": { "$gt": 0 } }] } },
                { "$group": { "_id": null, "sum": { "$sum": "$outstanding_balance_cents" }, "count": { "$sum": 1 } } }
            ],
        }
    }];

    let mut cursor = customers(db).aggregate(pipeline).await?;
    let Some(result) = cursor.try_next().await? else {
        return Ok(StatsAggregateResult {
            total_customers: 0,
            total_balance_due_cents: 0,
            active_debtors_count: 0,
        });
    };

    // "total"'s single branch uses `$count`, whose result doc has a "count"
    // field but no "_id" — same shape `i64_from_facet_branch` already reads.
    let total_customers = i64_from_facet_branch(&result, "total", "count") as u64;
    let total_balance_due_cents = i64_from_facet_branch(&result, "debtors", "sum");
    let active_debtors_count = i64_from_facet_branch(&result, "debtors", "count") as u64;

    Ok(StatsAggregateResult {
        total_customers,
        total_balance_due_cents,
        active_debtors_count,
    })
}
