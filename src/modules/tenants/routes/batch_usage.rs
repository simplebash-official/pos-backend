use super::authorize::authorize;
use super::batch_usage_request::BatchUsageRequest;
use super::batch_usage_response::BatchUsageResponse;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::tenants::usage;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;

/// Usage of several shops in one call (the console's lists). Requires `X-Provision-Secret`.
#[utoipa::path(
    post,
    path = "/tenants/usage",
    tag = modules::TENANTS,
    request_body = BatchUsageRequest,
    responses(
        (status = 200, description = "Usage figures", body = ApiResponse<BatchUsageResponse>),
        (status = 400, description = "More than 100 shops", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "The endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn batch_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BatchUsageRequest>,
) -> AppResult<Json<ApiResponse<BatchUsageResponse>>> {
    authorize(&state, &headers)?;
    let ids: Vec<String> = body
        .tenant_ids
        .iter()
        .map(|i| i.trim().to_string())
        .collect();
    let items = usage::usage_for_many(&state.db, &ids).await?;
    Ok(Json(ApiResponse::success(
        BatchUsageResponse { items },
        "Shop usage",
    )))
}
