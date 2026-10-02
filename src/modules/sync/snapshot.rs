// `GET /api/sync/snapshot`: the tenant's live rows as canonical change
// records, in apply order, paged. Used to bootstrap a new device or to recover
// from `CURSOR_EXPIRED`. `asOfSeq` is fixed by the first page (the tenant's
// highest seq at that moment) and repeated in every page token, so the device
// resumes pulling from that seq afterwards; rows edited while it pages arrive
// again through the feed and merge idempotently.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::TryStreamExt;
use mongodb::bson::{Bson, doc};
use serde::{Deserialize, Serialize};

use crate::{
    clients::db::Db,
    core::error::{AppError, AppResult},
    domain::sync_v2::{ChangeOp, ChangeRecord, SnapshotResponse},
    modules::sync::{
        apply_mongo::num_i64,
        cloud_store::{self, coll},
        resources::{Phase, ordered},
        service::hydrate,
    },
};

pub(crate) const DEFAULT_SNAPSHOT_PAGE: i64 = 500;
pub(crate) const MAX_SNAPSHOT_PAGE: i64 = 1000;

/// Where the next page starts: which resource, after which key.
#[derive(Debug, Serialize, Deserialize)]
struct PageToken {
    as_of_seq: i64,
    resource_index: usize,
    after_key: Option<String>,
}

fn encode_token(token: &PageToken) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(token).unwrap_or_default())
}

fn decode_token(text: &str) -> AppResult<PageToken> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| AppError::validation("invalid snapshot page token"))?;
    serde_json::from_slice(&bytes).map_err(|_| AppError::validation("invalid snapshot page token"))
}

pub async fn snapshot(
    db: &Db,
    page: Option<&str>,
    page_size: Option<i64>,
) -> AppResult<SnapshotResponse> {
    let page_size = page_size
        .unwrap_or(DEFAULT_SNAPSHOT_PAGE)
        .clamp(1, MAX_SNAPSHOT_PAGE);
    let mut token = match page {
        Some(text) => decode_token(text)?,
        None => PageToken {
            as_of_seq: cloud_store::current_seq(db).await?,
            resource_index: 0,
            after_key: None,
        },
    };
    let as_of_seq = token.as_of_seq;

    let resources: Vec<_> = ordered().filter(|s| s.phase == Phase::P3a).collect();
    let mut changes: Vec<ChangeRecord> = Vec::new();

    while token.resource_index < resources.len() && (changes.len() as i64) < page_size {
        let spec = resources[token.resource_index];
        let want = page_size - changes.len() as i64;

        let mut filter = doc! { "deleted_at": Bson::Null };
        if let Some(after) = &token.after_key {
            filter.insert("key", doc! { "$gt": after });
        }
        let mut cursor = coll(db, spec.table)?
            .find(filter)
            .sort(doc! { "key": 1 })
            .limit(want)
            .await?;
        let mut docs = Vec::new();
        while let Some(row) = cursor.try_next().await? {
            docs.push(row);
        }
        let fetched = docs.len() as i64;

        let payloads = hydrate(db, spec.name, docs.clone()).await?;
        if payloads.len() != docs.len() {
            return Err(AppError::internal(
                "snapshot hydration changed the number of rows",
            ));
        }
        for (row, payload) in docs.iter().zip(payloads) {
            let key = row.get_str("key").unwrap_or_default().to_string();
            token.after_key = Some(key.clone());
            changes.push(ChangeRecord {
                resource: spec.name.to_string(),
                key,
                op: ChangeOp::Upsert,
                version: num_i64(row, "version").unwrap_or(1),
                updated_at: row
                    .get_datetime("updated_at")
                    .map(|d| d.to_chrono())
                    .unwrap_or_default(),
                device_id: row
                    .get_str("updated_by_device")
                    .unwrap_or("cloud")
                    .to_string(),
                payload: Some(payload),
            });
        }

        if fetched < want {
            // This resource is exhausted: continue with the next one.
            token.resource_index += 1;
            token.after_key = None;
        }
    }

    let next_page = (token.resource_index < resources.len()).then(|| encode_token(&token));
    Ok(SnapshotResponse {
        as_of_seq,
        changes,
        next_page,
    })
}
