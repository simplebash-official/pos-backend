use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use mongodb::bson::{DateTime as BsonDateTime, doc, oid::ObjectId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    app::AppState,
    core::{
        constants::codes, error::AppError, middleware::auth::verify_bearer, response::ErrorResponse,
    },
};

/// Largest request body this middleware will buffer (to hash it and replay
/// it to the handler). Matches axum's default `DefaultBodyLimit`; the only
/// route that legitimately takes more (`/api/backup`) is exempt below. A
/// bigger body is answered 413 without being read into memory.
pub const MAX_IDEMPOTENT_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Client-supplied idempotency key string.
    pub key: String,
    /// User ID of the caller submitting the idempotent request.
    pub user_id: String,
    /// SHA-256 hash of the request method, URI, and payload.
    pub request_hash: String,
    /// State of processing ("in_progress" | "completed").
    pub status: String,
    /// HTTP status code of cached response, once completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_status: Option<u16>,
    /// Raw response body string of cached response, once completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    /// Timestamp when idempotency record was created.
    pub created_at: BsonDateTime,
}

/// How long an `in_progress` record blocks a retry with the same key before
/// this middleware treats it as abandoned and lets a new attempt take over.
///
/// The normal cleanup path (delete-on-5xx, a few lines below) only runs when
/// `next.run()` actually returns a `Response` — a client disconnecting
/// mid-request (exactly the scenario the frontend's offline outbox is built
/// around: a cashier's connection drops mid-checkout, and the same
/// `Idempotency-Key` is retried once it returns) or the handler task dying
/// outright never produces one, so without this escape hatch the record
/// blocks every future retry with a 409 forever — the offline engine never
/// mints a new key for the same queued operation. 30s is comfortably longer
/// than `complete_sale`'s several sequential Mongo writes, the slowest
/// request this middleware guards.
const IN_PROGRESS_STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

fn compute_request_hash(method: &Method, uri: &str, body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(method.as_str().as_bytes());
    hasher.update(b":");
    hasher.update(uri.as_bytes());
    hasher.update(b":");
    hasher.update(body);
    hex::encode(hasher.finalize())
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

/// Routes whose responses must never be captured into `idempotency_keys`.
/// `/api/auth` is here because a login response body *is* the issued JWT —
/// persisting it would leave bearer tokens sitting in plaintext in Mongo and
/// let anyone replaying the same `Idempotency-Key` read one back. Replaying a
/// login is meaningless anyway, so the whole route family opts out rather
/// than only redacting the body. The same goes for `/api/system/setup` (its
/// response carries the new admin's JWT) and `/api/backup` (an export is the
/// whole database, password hashes included, and imports are up to 50 MiB).
fn is_idempotency_exempt(path: &str) -> bool {
    // Kept as literals rather than built from `constants::modules` so this
    // stays allocation-free on the hot path for every mutating request.
    path == "/api/auth"
        || path.starts_with("/api/auth/")
        // Sync has its own retry-safe protocol (batch ids, idempotent apply).
        || path == "/api/sync"
        || path.starts_with("/api/sync/")
        || path == "/api/system/setup"
        || path == "/api/backup"
        || path.starts_with("/api/backup/")
}

/// Identifies the bucket an `Idempotency-Key` is scoped to: the verified
/// caller's id (local HS256 or identity-service token, via the same
/// `verify_bearer` every handler uses).
///
/// - `Ok(Some(id))` — a valid token.
/// - `Ok(None)` — no Bearer token: the request bypasses the idempotency store
///   entirely. Every route this middleware can reach that is not public
///   rejects it a moment later anyway, and an unauthenticated caller must not
///   be able to make the server buffer bodies or write records.
/// - `Err` (→ 401) — a *present but unverifiable* Bearer token, rather than a
///   silent fallback that could let a forged token reach a bucket it has no
///   claim to. A non-`Bearer` `Authorization` header counts as "no token".
async fn extract_user_id(
    headers: &axum::http::HeaderMap,
    state: &AppState,
) -> Result<Option<String>, AppError> {
    let has_bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("Bearer "));
    if !has_bearer {
        return Ok(None);
    }
    let verified = verify_bearer(headers, &state.config).await?;
    Ok(Some(verified.user_id))
}

pub async fn handle_idempotency(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let is_mutating = method == Method::POST
        || method == Method::PUT
        || method == Method::PATCH
        || method == Method::DELETE;

    if !is_mutating || is_idempotency_exempt(request.uri().path()) {
        return next.run(request).await;
    }

    let idempotency_key = request
        .headers()
        .get("idempotency-key")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let Some(key) = idempotency_key else {
        return next.run(request).await;
    };

    // Cloned so no borrow of the (non-`Sync`) request is held across the
    // await below.
    let headers = request.headers().clone();
    let user_id = match extract_user_id(&headers, &state).await {
        Ok(Some(id)) => id,
        Ok(None) => return next.run(request).await,
        Err(err) => return err.into_response(),
    };
    let uri = request.uri().to_string();

    let (parts, body) = request.into_parts();
    let body_bytes = match axum::body::to_bytes(body, MAX_IDEMPOTENT_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let err_resp = ErrorResponse::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                codes::VALIDATION_ERROR,
                format!("Request body exceeds the {MAX_IDEMPOTENT_BODY_BYTES}-byte limit"),
            );
            return (StatusCode::PAYLOAD_TOO_LARGE, axum::Json(err_resp)).into_response();
        }
    };

    let request_hash = compute_request_hash(&parts.method, &uri, &body_bytes);

    let existing_record = match &state.db {
        crate::clients::db::Db::Mongo(mongo_db) => {
            let collection = mongo_db.collection::<IdempotencyDocument>("idempotency_keys");
            match collection
                .find_one(doc! { "key": &key, "user_id": &user_id })
                .await
            {
                Ok(res) => res.map(|d| {
                    (
                        d.status,
                        d.request_hash,
                        d.response_status,
                        d.response_body,
                        d.created_at.to_chrono(),
                    )
                }),
                Err(err) => {
                    return AppError::internal(format!(
                        "Database error checking idempotency key: {err}"
                    ))
                    .into_response();
                }
            }
        }
        crate::clients::db::Db::Sqlite(pool) => {
            use sqlx::Row;
            let row = sqlx::query(
                "SELECT status, request_hash, response_status, response_body, created_at FROM idempotency_keys WHERE key = $1 AND user_id = $2",
            )
            .bind(&key)
            .bind(&user_id)
            .fetch_optional(pool)
            .await;

            match row {
                Ok(Some(r)) => {
                    let created_str: String = r.get("created_at");
                    let status: String = r.get("status");
                    let req_hash: String = r.get("request_hash");
                    let resp_status: Option<i64> = r.get("response_status");
                    let resp_body: Option<String> = r.get("response_body");
                    let created_at = crate::clients::sqlite::parse_iso_datetime(&created_str);
                    Some((
                        status,
                        req_hash,
                        resp_status.map(|s| s as u16),
                        resp_body,
                        created_at,
                    ))
                }
                Ok(None) => None,
                Err(err) => {
                    return AppError::internal(format!(
                        "Database error checking idempotency key: {err}"
                    ))
                    .into_response();
                }
            }
        }
    };

    if let Some((status, hash, resp_status, resp_body, created_at)) = existing_record {
        if status == "completed" {
            if hash != request_hash {
                let err_resp = ErrorResponse::with_details(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    codes::IDEMPOTENCY_KEY_REUSED,
                    "Idempotency-Key was already used with a different request payload.",
                    serde_json::json!({
                        "idempotencyKey": key
                    }),
                );
                return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(err_resp)).into_response();
            }

            let status_code = resp_status
                .and_then(|s| StatusCode::from_u16(s).ok())
                .unwrap_or(StatusCode::OK);

            let body_str = resp_body.unwrap_or_default();
            let response = Response::builder()
                .status(status_code)
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-replayed", "true")
                .body(Body::from(body_str))
                .unwrap_or_else(|_| (StatusCode::OK, Body::empty()).into_response());

            return response;
        } else if status == "in_progress" {
            let age = chrono::Utc::now().signed_duration_since(created_at);
            if age < chrono::Duration::seconds(IN_PROGRESS_STALE_AFTER.as_secs() as i64) {
                let err_resp = ErrorResponse::new(
                    StatusCode::CONFLICT,
                    codes::IDEMPOTENCY_IN_PROGRESS,
                    "A request with this Idempotency-Key is currently in progress. Please retry with backoff.",
                );
                let mut response = (StatusCode::CONFLICT, axum::Json(err_resp)).into_response();
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
                return response;
            }
            // Abandoned — clear it and fall through to the insert below
            match &state.db {
                crate::clients::db::Db::Mongo(mongo_db) => {
                    let collection = mongo_db.collection::<IdempotencyDocument>("idempotency_keys");
                    let _ = collection
                        .delete_one(
                            doc! { "key": &key, "user_id": &user_id, "status": "in_progress" },
                        )
                        .await;
                }
                crate::clients::db::Db::Sqlite(pool) => {
                    let _ = sqlx::query("DELETE FROM idempotency_keys WHERE key = $1 AND user_id = $2 AND status = 'in_progress'")
                        .bind(&key)
                        .bind(&user_id)
                        .execute(pool)
                        .await;
                }
            }
        }
    }

    match &state.db {
        crate::clients::db::Db::Mongo(mongo_db) => {
            let collection = mongo_db.collection::<IdempotencyDocument>("idempotency_keys");
            let insert_doc = IdempotencyDocument {
                id: None,
                key: key.clone(),
                user_id: user_id.clone(),
                request_hash: request_hash.clone(),
                status: "in_progress".to_string(),
                response_status: None,
                response_body: None,
                created_at: BsonDateTime::now(),
            };

            if let Err(err) = collection.insert_one(insert_doc).await {
                if let Ok(Some(record)) = collection
                    .find_one(doc! { "key": &key, "user_id": &user_id })
                    .await
                    && record.status == "in_progress"
                {
                    let err_resp = ErrorResponse::new(
                        StatusCode::CONFLICT,
                        codes::IDEMPOTENCY_IN_PROGRESS,
                        "A request with this Idempotency-Key is currently in progress. Please retry with backoff.",
                    );
                    let mut response = (StatusCode::CONFLICT, axum::Json(err_resp)).into_response();
                    response
                        .headers_mut()
                        .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
                    return response;
                }
                return AppError::internal(format!("Failed to register idempotency key: {err}"))
                    .into_response();
            }
        }
        crate::clients::db::Db::Sqlite(pool) => {
            let now_str = crate::clients::sqlite::now_utc_iso();
            let insert_res = sqlx::query(
                "INSERT INTO idempotency_keys (key, user_id, request_hash, status, created_at) VALUES ($1, $2, $3, 'in_progress', $4)",
            )
            .bind(&key)
            .bind(&user_id)
            .bind(&request_hash)
            .bind(&now_str)
            .execute(pool)
            .await;

            if let Err(_err) = insert_res {
                let err_resp = ErrorResponse::new(
                    StatusCode::CONFLICT,
                    codes::IDEMPOTENCY_IN_PROGRESS,
                    "A request with this Idempotency-Key is currently in progress. Please retry with backoff.",
                );
                let mut response = (StatusCode::CONFLICT, axum::Json(err_resp)).into_response();
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
                return response;
            }
        }
    }

    let new_request = Request::from_parts(parts, Body::from(body_bytes));
    let response = next.run(new_request).await;
    let status = response.status();

    let (resp_parts, resp_body) = response.into_parts();
    let resp_bytes = match axum::body::to_bytes(resp_body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => Bytes::new(),
    };

    if status.is_server_error() {
        // Remove in-progress record on 5xx so caller can retry cleanly
        match &state.db {
            crate::clients::db::Db::Mongo(mongo_db) => {
                let collection = mongo_db.collection::<IdempotencyDocument>("idempotency_keys");
                let _ = collection
                    .delete_one(doc! { "key": &key, "user_id": &user_id })
                    .await;
            }
            crate::clients::db::Db::Sqlite(pool) => {
                let _ = sqlx::query("DELETE FROM idempotency_keys WHERE key = $1 AND user_id = $2")
                    .bind(&key)
                    .bind(&user_id)
                    .execute(pool)
                    .await;
            }
        }
    } else {
        let body_str = String::from_utf8_lossy(&resp_bytes).to_string();
        match &state.db {
            crate::clients::db::Db::Mongo(mongo_db) => {
                let collection = mongo_db.collection::<IdempotencyDocument>("idempotency_keys");
                let _ = collection
                    .update_one(
                        doc! { "key": &key, "user_id": &user_id },
                        doc! {
                            "$set": {
                                "status": "completed",
                                "response_status": status.as_u16() as i32,
                                "response_body": body_str,
                            }
                        },
                    )
                    .await;
            }
            crate::clients::db::Db::Sqlite(pool) => {
                let _ = sqlx::query(
                    "UPDATE idempotency_keys SET status = 'completed', response_status = $3, response_body = $4 WHERE key = $1 AND user_id = $2",
                )
                .bind(&key)
                .bind(&user_id)
                .bind(status.as_u16() as i64)
                .bind(&body_str)
                .execute(pool)
                .await;
            }
        }
    }

    Response::from_parts(resp_parts, Body::from(resp_bytes))
}
