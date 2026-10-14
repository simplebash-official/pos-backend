// Read-only usage figures for one shop, for the operator console (`cloud/admin-backend`): how
// much of the POS a shop actually uses. Counts and sales totals only - never customer names,
// line items or any other record content. Every query runs inside the shop's own tenant scope,
// so the tenant filter is applied by `TenantDatabase` like for any request of that shop; MongoDB
// only (the directory this belongs to exists only in the multi-tenant cloud).

use futures_util::{StreamExt, TryStreamExt, stream};
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    clients::db::Db,
    core::{
        constants::codes,
        error::{AppError, AppResult},
        tenancy::{Tenant, with_tenant},
    },
    modules::{sync::cloud_store, tenants::repository},
};

const DAY_MS: i64 = 24 * 3600 * 1000;
/// Most shops one batch call may ask about, and how many are read at once.
pub const MAX_BATCH: usize = 100;
const CONCURRENCY: usize = 8;

/// One day of the "bills per day" chart (Sri Lanka calendar day).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DayUsage {
    /// `YYYY-MM-DD`, Asia/Colombo.
    pub date: String,
    pub bills: u64,
    pub sales_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantUsage {
    pub tenant_id: String,
    pub shop_code: String,
    pub name: String,
    pub suspended: bool,
    /// Invoices that were not voided, ever / in the last 30 days / in the last 7 days.
    pub bills_total: u64,
    pub bills_30d: u64,
    pub bills_7d: u64,
    /// Gross sales of those invoices in cents (credit notes are not subtracted).
    pub sales_total_cents: i64,
    pub sales_30d_cents: i64,
    /// ISO 8601 time of the newest non-voided invoice.
    pub last_bill_at: Option<String>,
    pub products: u64,
    pub customers: u64,
    /// Active staff accounts.
    pub users: u64,
    /// Registered desktop devices that were not revoked.
    pub devices: u64,
    /// ISO 8601 time of the most recent push or pull by any device.
    pub last_sync_at: Option<String>,
    /// The last 30 days, days without bills left out.
    pub bills_by_day: Vec<DayUsage>,
}

fn iso(dt: BsonDateTime) -> String {
    dt.to_chrono().to_rfc3339()
}

fn int(d: &Document, key: &str) -> i64 {
    d.get_i32(key)
        .map(i64::from)
        .or_else(|_| d.get_i64(key))
        .unwrap_or(0)
}

async fn first(cursor: mongodb::Cursor<Document>) -> AppResult<Option<Document>> {
    Ok(cursor.try_collect::<Vec<_>>().await?.into_iter().next())
}

/// Usage of one shop; 404 `TENANT_NOT_FOUND` if it is not in the directory.
pub async fn usage_for(db: &Db, tenant_id: &str) -> AppResult<TenantUsage> {
    let tenant = repository::find_tenant_by_key(db, tenant_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code(
                format!("No shop with id '{tenant_id}'"),
                codes::TENANT_NOT_FOUND,
            )
        })?;
    let mongo = db
        .as_mongo()
        .ok_or_else(|| AppError::internal("shop usage requires MongoDB"))?;
    let scope = Tenant::id(tenant_id)?;

    let now_ms = BsonDateTime::now().timestamp_millis();
    let days_ago = |days: i64| BsonDateTime::from_millis(now_ms - days * DAY_MS);

    with_tenant(scope, async {
        // Invoices: one pass for every figure. Voided ones never count.
        let invoices = mongo.collection::<Document>("invoices");
        let facets = first(
            invoices
                .aggregate(vec![
                    doc! { "$match": { "status": { "$ne": "voided" } } },
                    doc! { "$facet": {
                        "all": [{ "$group": {
                            "_id": null,
                            "n": { "$sum": 1 },
                            "cents": { "$sum": "$total_cents" },
                            "last": { "$max": "$created_at" },
                        } }],
                        "d30": [
                            { "$match": { "created_at": { "$gte": days_ago(30) } } },
                            { "$group": { "_id": null, "n": { "$sum": 1 }, "cents": { "$sum": "$total_cents" } } },
                        ],
                        "d7": [
                            { "$match": { "created_at": { "$gte": days_ago(7) } } },
                            { "$count": "n" },
                        ],
                        "by_day": [
                            { "$match": { "created_at": { "$gte": days_ago(30) } } },
                            { "$group": {
                                "_id": { "$dateToString": { "format": "%Y-%m-%d", "date": "$created_at", "timezone": "Asia/Colombo" } },
                                "n": { "$sum": 1 },
                                "cents": { "$sum": "$total_cents" },
                            } },
                            { "$sort": { "_id": 1 } },
                        ],
                    } },
                ])
                .await?,
        )
        .await?
        .unwrap_or_default();
        let one = |name: &str| -> Document {
            facets
                .get_array(name)
                .ok()
                .and_then(|a| a.first())
                .and_then(|b| b.as_document())
                .cloned()
                .unwrap_or_default()
        };
        let (all, d30, d7) = (one("all"), one("d30"), one("d7"));
        let by_day: Vec<DayUsage> = facets
            .get_array("by_day")
            .map(|a| {
                a.iter()
                    .filter_map(|b| b.as_document())
                    .filter_map(|d| {
                        Some(DayUsage {
                            date: d.get_str("_id").ok()?.to_string(),
                            bills: int(d, "n") as u64,
                            sales_cents: int(d, "cents"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let products = mongo
            .collection::<Document>("products")
            .count_documents(doc! { "deleted_at": null })
            .await?;
        let customers = mongo
            .collection::<Document>("customers")
            .count_documents(doc! { "deleted_at": null })
            .await?;
        let users = mongo
            .collection::<Document>("users")
            .count_documents(doc! { "is_active": true })
            .await?;

        let devices = first(
            mongo
                .collection::<Document>(cloud_store::DEVICES)
                .aggregate(vec![
                    doc! { "$match": { "revoked": { "$ne": true } } },
                    doc! { "$group": { "_id": null, "n": { "$sum": 1 }, "last": { "$max": "$last_seen" } } },
                ])
                .await?,
        )
        .await?
        .unwrap_or_default();

        Ok(TenantUsage {
            tenant_id: tenant.key.clone(),
            shop_code: tenant.shop_code.clone(),
            name: tenant.name.clone(),
            suspended: tenant.suspended_at.is_some(),
            bills_total: int(&all, "n") as u64,
            bills_30d: int(&d30, "n") as u64,
            bills_7d: int(&d7, "n") as u64,
            sales_total_cents: int(&all, "cents"),
            sales_30d_cents: int(&d30, "cents"),
            last_bill_at: all.get_datetime("last").ok().copied().map(iso),
            products,
            customers,
            users,
            devices: int(&devices, "n") as u64,
            last_sync_at: devices.get_datetime("last").ok().copied().map(iso),
            bills_by_day: by_day,
        })
    })
    .await
}

/// Usage of several shops at once (a shop that is not in the directory is simply left out), a few
/// reads at a time so one call cannot flood MongoDB.
pub async fn usage_for_many(db: &Db, tenant_ids: &[String]) -> AppResult<Vec<TenantUsage>> {
    if tenant_ids.len() > MAX_BATCH {
        return Err(AppError::validation(format!(
            "at most {MAX_BATCH} shops per request"
        )));
    }
    // Futures are built first and polled a few at a time; mapping inside the stream trips a
    // higher-ranked lifetime error once the handler is boxed by `routes!`.
    let pending: Vec<_> = tenant_ids.iter().map(|id| usage_for(db, id)).collect();
    let results: Vec<AppResult<TenantUsage>> =
        stream::iter(pending).buffered(CONCURRENCY).collect().await;
    let mut out = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(usage) => out.push(usage),
            Err(AppError::NotFound { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}
