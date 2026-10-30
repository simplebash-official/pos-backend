use super::hydrate::hydrate;
use super::syncable::SYNCABLE;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::sync::{ResourceChanges, SyncChangesQuery, SyncChangesResponse};

use crate::modules::sync::cursor::{DecodedCursor, decode_cursor, encode_cursor};
use crate::modules::sync::repository;
use chrono::Utc;
use mongodb::bson::Document;
use serde_json::Value;
use std::collections::HashMap;

const DEFAULT_LIMIT: u64 = 500;

const MAX_LIMIT: u64 = 500;

/// Collections whose REST routes are Admin-only (`AdminUser` on every
/// suppliers / supplier-products / purchases route — pricing and cost data).
/// The delta feed must not hand them to anyone the REST API would refuse.
const ADMIN_ONLY_COLLECTIONS: &[&str] = &["suppliers", "supplier_products", "purchases"];

/// Maps a caller-supplied resource name to its collection, accepting the
/// snake_case spelling of the two multi-word names as an alias.
fn collection_for(resource: &str) -> Option<&'static str> {
    SYNCABLE
        .iter()
        .find(|(name, collection)| {
            *name == resource
                || (resource == "supplier_products" && *collection == "supplier_products")
                || (resource == "stock_movements" && *collection == "stock_movements")
                || (resource == "stock_batches" && *collection == "stock_batches")
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

/// `caller_is_admin == false` silently drops the Admin-only resources
/// (`ADMIN_ONLY_COLLECTIONS`) from the response, the same way an unknown
/// resource name is skipped.
pub async fn get_changes(
    db: &Db,
    query: SyncChangesQuery,
    caller_is_admin: bool,
) -> AppResult<SyncChangesResponse> {
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
        if !caller_is_admin && ADMIN_ONLY_COLLECTIONS.contains(&collection_name) {
            continue;
        }

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
