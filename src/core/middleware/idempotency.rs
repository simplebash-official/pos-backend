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
    core::{constants::codes, error::AppError, middleware::auth::Claims, response::ErrorResponse},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub user_id: String,
    pub request_hash: String,
    pub status: String, // "in_progress" | "completed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    pub created_at: BsonDateTime,
}

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
/// than only redacting the body.
fn is_idempotency_exempt(path: &str) -> bool {
    // Kept as a literal rather than built from `constants::modules::AUTH` so
    // this stays allocation-free on the hot path for every mutating request.
    path == "/api/auth" || path.starts_with("/api/auth/")
}

/// Identifies the bucket an `Idempotency-Key` is scoped to. A valid token
/// gives the account's id; an unauthenticated caller falls back to its device
/// id, then to a shared `"anonymous"` bucket.
///
/// A *present but unverifiable* Bearer token is an error rather than a
/// fallback (`Err` → 401): silently downgrading it to the device or anonymous
/// bucket would let a caller with an expired or forged token land in — and
/// read cached responses out of — a bucket it has no claim to. Every route
/// this middleware can reach requires auth anyway (see the intentional-public
/// list in `app::build_router`), so the handler would reject such a request a
/// moment later regardless; rejecting here just avoids writing a record for
/// it first. A non-`Bearer` `Authorization` header is left to fall through,
/// since it was never a token claim to begin with.
fn extract_user_id(request: &Request, jwt_secret: &str) -> Result<String, AppError> {
    if let Some(auth_header) = request.headers().get(header::AUTHORIZATION)
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str.strip_prefix("Bearer ")
    {
        return match jsonwebtoken::decode::<Claims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_bytes()),
            &jsonwebtoken::Validation::default(),
        ) {
            Ok(data) => Ok(data.claims.sub),
            Err(err) => Err(AppError::from(err)),
        };
    }

    if let Some(device_header) = request.headers().get("x-device-id")
        && let Ok(device_str) = device_header.to_str()
        && !device_str.trim().is_empty()
    {
        return Ok(format!("device:{}", device_str.trim()));
    }

    Ok("anonymous".to_string())
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

    let user_id = match extract_user_id(&request, &state.config.jwt_secret) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };
    let uri = request.uri().to_string();

    let (parts, body) = request.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => {
            return AppError::validation(format!("Failed to read request body: {err}"))
                .into_response();
        }
    };

    let request_hash = compute_request_hash(&parts.method, &uri, &body_bytes);
    let collection = state
        .db
        .collection::<IdempotencyDocument>("idempotency_keys");

    let existing = match collection
        .find_one(doc! { "key": &key, "user_id": &user_id })
        .await
    {
        Ok(res) => res,
        Err(err) => {
            return AppError::internal(format!("Database error checking idempotency key: {err}"))
                .into_response();
        }
    };

    if let Some(record) = existing {
        if record.status == "completed" {
            if record.request_hash != request_hash {
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

            let status_code = record
                .response_status
                .and_then(|s| StatusCode::from_u16(s).ok())
                .unwrap_or(StatusCode::OK);

            let body_str = record.response_body.unwrap_or_default();
            let response = Response::builder()
                .status(status_code)
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-replayed", "true")
                .body(Body::from(body_str))
                .unwrap_or_else(|_| (StatusCode::OK, Body::empty()).into_response());

            return response;
        } else if record.status == "in_progress" {
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
        // Race condition: another concurrent request may have inserted first
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
        let _ = collection
            .delete_one(doc! { "key": &key, "user_id": &user_id })
            .await;
    } else {
        let body_str = String::from_utf8_lossy(&resp_bytes).to_string();
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

    Response::from_parts(resp_parts, Body::from(resp_bytes))
}
