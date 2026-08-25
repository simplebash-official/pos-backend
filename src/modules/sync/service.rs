use chrono::Utc;
use mongodb::{Database, bson::Document};
use serde_json::Value;
use std::collections::HashMap;

use crate::{
    core::error::AppResult,
    domain::sync::{
        ResourceChanges, ResourceSyncStatus, SyncChangesQuery, SyncChangesResponse,
        SyncStatusResponse,
    },
    modules::{
        billing::service::{
            self as billing_service, credit_notes as billing_credit_notes_service,
            payments as billing_payments_service,
        },
        customers::service as customers_service,
        employees::service as employees_service,
        inventory::service::{
            category as inventory_category, product as inventory_product,
            product_serial as inventory_product_serial, stock as inventory_stock,
        },
        print_jobs::service as print_jobs_service,
        purchases::service::purchase as purchases_service,
        repairs::service as repairs_service,
        supplier_products::service::link as supplier_products_service,
        suppliers::service as suppliers_service,
        sync::{
            cursor::{DecodedCursor, decode_cursor, encode_cursor},
            repository,
        },
    },
};

const DEFAULT_LIMIT: u64 = 500;
const MAX_LIMIT: u64 = 500;

/// The resources a client may sync, paired with the collection each reads
/// from. `subcategories` is deliberately absent: a subcategory is not an
/// independent entity to the client, it is a field on the category it
/// belongs to, and `hydrate` folds it in there (see
/// `inventory::service::category::hydrate_sync_documents`).
const SYNCABLE: &[(&str, &str)] = &[
    ("products", "products"),
    ("categories", "categories"),
    ("suppliers", "suppliers"),
    ("supplierProducts", "supplier_products"),
    ("purchases", "purchases"),
    ("stockMovements", "stock_movements"),
    ("customers", "customers"),
    ("employees", "employees"),
    ("repairs", "repairs"),
    ("printJobs", "print_jobs"),
    ("invoices", "invoices"),
    ("payments", "payments"),
    ("creditNotes", "credit_notes"),
    ("productSerials", "product_serials"),
];

/// Maps a caller-supplied resource name to its collection, accepting the
/// snake_case spelling of the two multi-word names as an alias.
fn collection_for(resource: &str) -> Option<&'static str> {
    SYNCABLE
        .iter()
        .find(|(name, collection)| {
            *name == resource
                || (resource == "supplier_products" && *collection == "supplier_products")
                || (resource == "stock_movements" && *collection == "stock_movements")
                || (resource == "print_jobs" && *collection == "print_jobs")
        })
        .map(|(_, collection)| *collection)
}

fn parse_cursors_map(query: &SyncChangesQuery) -> AppResult<HashMap<String, String>> {
    let mut map = HashMap::new();

    if let Some(ref cursors_raw) = query.cursors {
        let trimmed = cursors_raw.trim();
        if trimmed.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(trimmed) {
                map.extend(parsed);
            }
        } else {
            for pair in trimmed.split(',') {
                if let Some((k, v)) = pair.split_once('=') {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
    }

    if let Some(ref since) = query.since
        && let Some(ref resources) = query.resources
    {
        for r in resources.split(',') {
            let name = r.trim();
            if !name.is_empty() && !map.contains_key(name) {
                map.insert(name.to_string(), since.clone());
            }
        }
    }

    Ok(map)
}

/// Turns a page of raw documents into the same DTOs the REST read endpoints
/// return.
///
/// This is the whole point of the module. The client mirrors both feeds into
/// one local table, so a row arriving by delta must be indistinguishable
/// from the same row arriving in a snapshot — same camelCase field names,
/// same resolved display fields, no BSON `$oid`/`$date` wrappers. Serializing
/// the raw `Document` instead would write structurally different rows for the
/// same entity depending only on which feed happened to deliver it.
async fn hydrate(db: &Database, resource: &str, documents: Vec<Document>) -> AppResult<Vec<Value>> {
    let values = match resource {
        "products" => to_values(inventory_product::hydrate_sync_documents(db, documents).await?)?,
        "categories" => {
            to_values(inventory_category::hydrate_sync_documents(db, documents).await?)?
        }
        "suppliers" => to_values(suppliers_service::hydrate_sync_documents(documents)?)?,
        "supplierProducts" | "supplier_products" => to_values(
            supplier_products_service::hydrate_sync_documents(documents)?,
        )?,
        "purchases" => to_values(purchases_service::hydrate_sync_documents(db, documents).await?)?,
        "stockMovements" | "stock_movements" => {
            to_values(inventory_stock::hydrate_sync_documents(documents)?)?
        }
        "customers" => to_values(customers_service::hydrate_sync_documents(documents)?)?,
        // Unlike every other arm here, `employees::hydrate_sync_documents` is
        // async and takes `db` — it enriches each row with a live-resolved
        // `login` summary, the same cross-module lookup a REST read performs
        // (see `modules::employees::service::hydrate_sync_documents`).
        "employees" => to_values(employees_service::hydrate_sync_documents(db, documents).await?)?,
        "repairs" => to_values(repairs_service::hydrate_sync_documents(documents)?)?,
        "printJobs" | "print_jobs" => {
            to_values(print_jobs_service::hydrate_sync_documents(documents)?)?
        }
        "invoices" => to_values(billing_service::hydrate_sync_documents(documents)?)?,
        "payments" => to_values(billing_payments_service::hydrate_sync_documents(documents)?)?,
        "creditNotes" => to_values(billing_credit_notes_service::hydrate_sync_documents(
            documents,
        )?)?,
        "productSerials" => {
            to_values(inventory_product_serial::hydrate_sync_documents(documents)?)?
        }
        _ => Vec::new(),
    };
    Ok(values)
}

fn to_values<T: serde::Serialize>(items: Vec<T>) -> AppResult<Vec<Value>> {
    items
        .into_iter()
        .map(|item| serde_json::to_value(item).map_err(Into::into))
        .collect()
}

pub async fn get_changes(db: &Database, query: SyncChangesQuery) -> AppResult<SyncChangesResponse> {
    let server_time = Utc::now();
    let requested_limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    // `limit = 0` means "tell me the cursor, send me nothing".
    let cursor_only = requested_limit == 0;
    let limit = if cursor_only {
        0
    } else {
        requested_limit.clamp(1, MAX_LIMIT)
    };
    let cursors_map = parse_cursors_map(&query)?;

    let requested_resources: Vec<String> = match query.resources {
        Some(ref res_str) => res_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => SYNCABLE.iter().map(|(name, _)| name.to_string()).collect(),
    };

    let mut changes = HashMap::new();

    for resource in requested_resources {
        let Some(collection_name) = collection_for(&resource) else {
            continue;
        };

        let cursor_str_opt = cursors_map
            .get(&resource)
            .or_else(|| cursors_map.get(collection_name));
        let decoded_cursor: Option<DecodedCursor> = match cursor_str_opt {
            Some(s) if !s.is_empty() => Some(decode_cursor(s)?),
            _ => None,
        };
        let is_full = decoded_cursor.is_none();

        if cursor_only {
            let marker = repository::fetch_latest_change_marker(db, collection_name).await?;
            let next_cursor = marker.map(|(ts, key)| encode_cursor(ts.to_chrono(), &key));
            changes.insert(
                resource,
                serde_json::to_value(ResourceChanges::<Value> {
                    full: is_full,
                    items: Vec::new(),
                    deleted: Vec::new(),
                    next_cursor,
                    has_more: false,
                    unchanged: false,
                })?,
            );
            continue;
        }

        let raw_docs = repository::fetch_collection_changes(
            db,
            collection_name,
            decoded_cursor.as_ref(),
            (limit + 1) as i64,
        )
        .await?;

        let has_more = raw_docs.len() as u64 > limit;
        let docs = if has_more {
            &raw_docs[..limit as usize]
        } else {
            &raw_docs[..]
        };

        let mut live_docs: Vec<Document> = Vec::new();
        let mut deleted = Vec::new();

        let mut last_updated = decoded_cursor.as_ref().map(|c| c.timestamp);
        let mut last_key = decoded_cursor.as_ref().and_then(|c| c.key.clone());

        for doc in docs {
            let key = doc.get_str("key").unwrap_or_default().to_string();
            let is_deleted = doc
                .get("deleted_at")
                .is_some_and(|v| !matches!(v, mongodb::bson::Bson::Null));

            if let Ok(updated_dt) = doc.get_datetime("updated_at") {
                last_updated = Some(updated_dt.to_chrono());
                last_key = Some(key.clone());
            }

            if is_deleted {
                deleted.push(key);
            } else {
                live_docs.push(doc.clone());
            }
        }

        // Nothing moved: no rows came back and the cursor has nowhere to
        // advance to. Say so explicitly rather than echoing the caller's own
        // cursor back and leaving them to guess.
        let unchanged = !is_full && docs.is_empty();

        let items = hydrate(db, &resource, live_docs).await?;

        let next_cursor = match (last_updated, last_key) {
            (Some(ts), Some(k)) => Some(encode_cursor(ts, &k)),
            _ => cursor_str_opt.cloned(),
        };

        changes.insert(
            resource,
            serde_json::to_value(ResourceChanges {
                full: is_full,
                items,
                deleted,
                next_cursor,
                has_more,
                unchanged,
            })?,
        );
    }

    Ok(SyncChangesResponse {
        server_time,
        changes,
    })
}

pub async fn get_sync_status(db: &Database) -> AppResult<SyncStatusResponse> {
    let mut resources = HashMap::new();

    for (resource_name, collection_name) in SYNCABLE {
        if let Some((updated_at, key)) =
            repository::fetch_latest_change_marker(db, collection_name).await?
        {
            let timestamp = updated_at.to_chrono();
            resources.insert(
                resource_name.to_string(),
                ResourceSyncStatus {
                    last_updated_at: timestamp,
                    cursor: encode_cursor(timestamp, &key),
                },
            );
        }
    }

    Ok(SyncStatusResponse {
        server_time: Utc::now(),
        resources,
    })
}
