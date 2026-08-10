use chrono::Utc;
use mongodb::Database;
use serde_json::Value;
use std::collections::HashMap;

use crate::{
    core::error::AppResult,
    domain::sync::{ResourceChanges, SyncChangesQuery, SyncChangesResponse},
    modules::sync::{
        cursor::{DecodedCursor, decode_cursor, encode_cursor},
        repository,
    },
};

const DEFAULT_LIMIT: u64 = 500;
const MAX_LIMIT: u64 = 500;

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

pub async fn get_changes(db: &Database, query: SyncChangesQuery) -> AppResult<SyncChangesResponse> {
    let server_time = Utc::now();
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let cursors_map = parse_cursors_map(&query)?;

    let requested_resources: Vec<String> = if let Some(ref res_str) = query.resources {
        res_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![
            "products".to_string(),
            "categories".to_string(),
            "subcategories".to_string(),
            "suppliers".to_string(),
            "supplierProducts".to_string(),
            "purchases".to_string(),
            "stockMovements".to_string(),
        ]
    };

    let mut changes = HashMap::new();

    for resource in requested_resources {
        let collection_name = match resource.as_str() {
            "products" => "products",
            "categories" => "categories",
            "subcategories" => "subcategories",
            "suppliers" => "suppliers",
            "supplierProducts" | "supplier_products" => "supplier_products",
            "purchases" => "purchases",
            "stockMovements" | "stock_movements" => "stock_movements",
            _ => continue,
        };

        let cursor_str_opt = cursors_map
            .get(&resource)
            .or_else(|| cursors_map.get(collection_name));
        let decoded_cursor: Option<DecodedCursor> = match cursor_str_opt {
            Some(s) if !s.is_empty() => Some(decode_cursor(s)?),
            _ => None,
        };

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

        let is_full = decoded_cursor.is_none();
        let mut items = Vec::new();
        let mut deleted = Vec::new();

        let mut last_updated = decoded_cursor.as_ref().map(|c| c.timestamp);
        let mut last_key = decoded_cursor.as_ref().and_then(|c| c.key.clone());

        for doc in docs {
            let key = doc.get_str("key").unwrap_or_default().to_string();
            let is_deleted = doc
                .get("deleted_at")
                .is_some_and(|v| !matches!(v, mongodb::bson::Bson::Null));

            if let Ok(updated_dt) = doc.get_datetime("updated_at") {
                let chrono_dt = updated_dt.to_chrono();
                last_updated = Some(chrono_dt);
                last_key = Some(key.clone());
            }

            if is_deleted {
                deleted.push(key);
            } else {
                let serialized = serde_json::to_value(doc).unwrap_or(Value::Null);
                items.push(serialized);
            }
        }

        let next_cursor = match (last_updated, last_key) {
            (Some(ts), Some(k)) => Some(encode_cursor(ts, &k)),
            _ => cursor_str_opt.cloned(),
        };

        let resource_changes = ResourceChanges {
            full: is_full,
            items,
            deleted,
            next_cursor,
            has_more,
        };

        changes.insert(
            resource,
            serde_json::to_value(resource_changes).unwrap_or(Value::Null),
        );
    }

    Ok(SyncChangesResponse {
        server_time,
        changes,
    })
}
