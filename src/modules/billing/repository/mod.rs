// Mongo access for the `invoices`, `payments`, and `returns` collections. Same
// never-interpret-a-miss-as-an-error convention as every other repository
// in this codebase — `service` decides what a missing row means.

pub(crate) mod invoice;
pub(crate) mod returns;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use crate::{
    core::error::AppResult,
    modules::billing::model::{InvoiceDocument, PaymentDocument},
};

fn invoices(db: &Database) -> Collection<InvoiceDocument> {
    db.collection("invoices")
}

fn payments(db: &Database) -> Collection<PaymentDocument> {
    db.collection("payments")
}

pub(crate) async fn insert_invoice(
    db: &Database,
    mut document: InvoiceDocument,
) -> AppResult<InvoiceDocument> {
    let result = invoices(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

pub(crate) async fn find_invoice_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<InvoiceDocument>> {
    Ok(invoices(db).find_one(doc! { "_id": id }).await?)
}

pub(crate) async fn find_invoice_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<InvoiceDocument>> {
    Ok(invoices(db).find_one(doc! { "key": key }).await?)
}

pub(crate) async fn find_invoice_by_id_or_key(
    db: &Database,
    id_or_key: &str,
) -> AppResult<Option<InvoiceDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_invoice_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_invoice_by_key(db, id_or_key).await
}

pub(crate) async fn list_invoices(
    db: &Database,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<InvoiceDocument>, u64)> {
    let total = invoices(db).count_documents(filter.clone()).await?;

    let mut cursor = invoices(db)
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok((items, total))
}

/// The one narrow mutator invoices ever get after creation — flips
/// `status` (and, for a cancellation, the `cancelled_*` fields) via `$set`.
/// No general "update an invoice" repository function exists (D3).
pub(crate) async fn update_invoice_status(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<InvoiceDocument>> {
    Ok(invoices(db)
        .find_one_and_update(
            doc! { "_id": id },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await?)
}

pub(crate) async fn insert_payment(
    db: &Database,
    mut document: PaymentDocument,
) -> AppResult<PaymentDocument> {
    let result = payments(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

pub(crate) async fn list_payments_for_invoice(
    db: &Database,
    invoice_key: &str,
) -> AppResult<Vec<PaymentDocument>> {
    let mut cursor = payments(db)
        .find(doc! { "invoice_key": invoice_key })
        .sort(doc! { "recorded_at": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Used by `service::sale::cancel_invoice`'s guard (D8): cancellation is
/// blocked once more than the original sale-time payment(s) exist.
pub(crate) async fn count_payments_for_invoice(db: &Database, invoice_key: &str) -> AppResult<u64> {
    Ok(payments(db)
        .count_documents(doc! { "invoice_key": invoice_key })
        .await?)
}

/// Everything `service::get_billing_stats` needs from `invoices`, computed in
/// one aggregation round-trip.
pub(crate) struct StatsAggregateResult {
    /// Sum of `total_cents` for invoices created in `[today_start, today_end)`.
    pub today_sales_cents: i64,
    /// Count of invoices created in that same window.
    pub today_invoice_count: u64,
    /// Sum of `total_cents` across ALL invoices, unscoped by date, where
    /// `is_credit == true` or `status == "pending"`.
    pub outstanding_credit_cents: i64,
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

/// Runs a single `$facet` aggregation over `invoices` to compute the Sales &
/// Invoices History screen's 4 KPI cards in one database round-trip:
/// today's sales sum + count, and the all-time outstanding-credit sum.
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
                        "created_at": {
                            "$gte": BsonDateTime::from_chrono(today_start),
                            "$lt": BsonDateTime::from_chrono(today_end),
                        }
                    }
                },
                { "$group": { "_id": null, "sum": { "$sum": "$total_cents" }, "count": { "$sum": 1 } } }
            ],
            "outstanding": [
                { "$match": { "$or": [ { "is_credit": true }, { "status": "pending" } ] } },
                { "$group": { "_id": null, "sum": { "$sum": "$total_cents" } } }
            ],
        }
    }];

    let mut cursor = invoices(db).aggregate(pipeline).await?;
    let Some(result) = cursor.try_next().await? else {
        return Ok(StatsAggregateResult {
            today_sales_cents: 0,
            today_invoice_count: 0,
            outstanding_credit_cents: 0,
        });
    };

    let today_sales_cents = i64_from_facet_branch(&result, "today", "sum");
    let today_invoice_count = i64_from_facet_branch(&result, "today", "count") as u64;
    let outstanding_credit_cents = i64_from_facet_branch(&result, "outstanding", "sum");

    Ok(StatsAggregateResult {
        today_sales_cents,
        today_invoice_count,
        outstanding_credit_cents,
    })
}
