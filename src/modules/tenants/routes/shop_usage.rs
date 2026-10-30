use super::authorize::authorize;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::tenants::usage;
use crate::modules::tenants::usage::TenantUsage;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

/// How much one shop uses the POS: bill counts, gross sales, catalogue and staff sizes, devices
/// and the last sync. Counts and totals only, no record content. Requires `X-Provision-Secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/usage",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    responses(
        (status = 200, description = "Usage figures", body = ApiResponse<TenantUsage>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn shop_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> AppResult<Json<ApiResponse<TenantUsage>>> {
    authorize(&state, &headers)?;
    let usage = usage::usage_for(&state.db, tenant_id.trim()).await?;
    Ok(Json(ApiResponse::success(usage, "Shop usage")))
}
