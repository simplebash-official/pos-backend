// Mongo access for the `repairs` collection only. Same never-interpret-a-
// miss-as-an-error convention as every other repository in this codebase —
// `service` decides what a missing row means.

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, modules::repairs::model::RepairDocument};

fn repairs(db: &Database) -> Collection<RepairDocument> {
    db.collection("repairs")
}

pub(crate) async fn find_by_id(db: &Database, id: ObjectId) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
        .await?)
}

pub(crate) async fn find_by_key(db: &Database, key: &str) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
        .await?)
}

pub(crate) async fn find_by_id_or_key(
    db: &Database,
    id_or_key: &str,
) -> AppResult<Option<RepairDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_by_key(db, id_or_key).await
}

pub(crate) async fn insert(
    db: &Database,
    mut document: RepairDocument,
) -> AppResult<RepairDocument> {
    let result = repairs(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

pub(crate) async fn update(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

/// Narrow status-only mutator, looked up by `key` — used by
/// `service::mark_delivered`, the cross-module hook `billing`'s
/// complete-sale flow calls. Bypasses the general `update` set-doc builder
/// since this is the one place a ticket's status changes without a client
/// PATCH body.
pub(crate) async fn set_status_by_key(
    db: &Database,
    key: &str,
    status: &str,
) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one_and_update(
            doc! { "key": key, "deleted_at": { "$exists": false } },
            doc! {
                "$set": { "status": status, "updated_at": BsonDateTime::now() },
                "$inc": { "version": 1 },
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete(
    db: &Database,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<RepairDocument>> {
    let now = BsonDateTime::now();
    let mut set_doc = doc! { "deleted_at": now, "updated_at": now };
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }
    Ok(repairs(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn list(
    db: &Database,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<RepairDocument>, u64)> {
    let mut effective_filter = filter;
    if !effective_filter.contains_key("deleted_at") {
        effective_filter.insert("deleted_at", doc! { "$exists": false });
    }

    let total = repairs(db)
        .count_documents(effective_filter.clone())
        .await?;

    let mut cursor = repairs(db)
        .find(effective_filter)
        .sort(doc! { "updated_at": -1 })
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok((items, total))
}

/// Everything `service::get_repair_stats` needs from `repairs`, computed in
/// one aggregation round-trip.
pub(crate) struct StatsAggregateResult {
    /// Count of non-deleted tickets created in `[today_start, today_end)`.
    pub today_job_count: u64,
    /// Sum of `estimated_cost_cents` (0 where unset) for those same tickets.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted tickets (any date) whose status is neither
    /// "delivered" nor "cancelled".
    pub pending_job_count: u64,
}

/// Extracts a scalar `i64` field pushed by a `$group` facet branch, out of
/// that branch's array-of-one-document shape (`[{ "_id": null, field: N }]`,
/// empty array if the branch matched nothing).
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

/// Runs a single `$facet` aggregation over `repairs` to compute the Repair
/// Jobs screen's KPI cards in one database round-trip: today's ticket count
/// + revenue sum, and the all-time open-ticket count.
pub(crate) async fn aggregate_stats(
    db: &Database,
    today_start: DateTime<Utc>,
    today_end: DateTime<Utc>,
) -> AppResult<StatsAggregateResult> {
    let pipeline = vec![doc! {
        "$facet": {
            "today": [
                {
                    "$match": {
                        "deleted_at": { "$exists": false },
                        "created_at": {
                            "$gte": BsonDateTime::from_chrono(today_start),
                            "$lt": BsonDateTime::from_chrono(today_end),
                        }
                    }
                },
                {
                    "$group": {
                        "_id": null,
                        "sum": { "$sum": { "$ifNull": ["$estimated_cost_cents", 0] } },
                        "count": { "$sum": 1 },
                    }
                }
            ],
            "pending": [
                {
                    "$match": {
                        "deleted_at": { "$exists": false },
                        "status": { "$nin": ["delivered", "cancelled"] },
                    }
                },
                { "$count": "count" }
            ],
        }
    }];

    let mut cursor = repairs(db).aggregate(pipeline).await?;
    let Some(result) = cursor.try_next().await? else {
        return Ok(StatsAggregateResult {
            today_job_count: 0,
            today_revenue_cents: 0,
            pending_job_count: 0,
        });
    };

    let today_job_count = i64_from_facet_branch(&result, "today", "count") as u64;
    let today_revenue_cents = i64_from_facet_branch(&result, "today", "sum");
    let pending_job_count = i64_from_facet_branch(&result, "pending", "count") as u64;

    Ok(StatsAggregateResult {
        today_job_count,
        today_revenue_cents,
        pending_job_count,
    })
}
