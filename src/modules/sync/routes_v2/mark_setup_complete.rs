use super::require_cloud::require_cloud;
use super::setup_complete_request::SetupCompleteRequest;
use super::setup_complete_response::SetupCompleteResponse;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};

use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::sync::cloud_store;
use crate::modules::tenants::repository as tenants_repository;
use axum::Json;
use axum::extract::State;

/// A device that finished the shop's first-time setup tells the cloud, so the
/// website's POS does not ask the same demo-vs-clean question again. Idempotent
/// and one-way: a shop that is already set up is left exactly as it is.
#[utoipa::path(post, path = "/setup-complete", tag = modules::SYNC,
    request_body = SetupCompleteRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The shop is marked as set up (or already was)", body = ApiResponse<SetupCompleteResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token or revoked device", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn mark_setup_complete(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<SetupCompleteRequest>,
) -> AppResult<Json<ApiResponse<SetupCompleteResponse>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    if !cloud_store::ensure_device_active(&state.db, &device_id).await? {
        return Err(AppError::forbidden_with_code(
            "This device has been revoked",
            "DEVICE_REVOKED",
        ));
    }
    let tenant_id = user
        .tenant_id
        .as_deref()
        .ok_or_else(|| AppError::unauthorized("Authenticated token has no tenant scope"))?;
    let tenant = tenants_repository::find_tenant_by_key(&state.db, tenant_id)
        .await?
        .ok_or_else(|| AppError::not_found("Shop not found"))?;
    let changed = !tenant.setup_completed;
    if changed {
        tenants_repository::complete_tenant_setup(&state.db, tenant_id, body.sample_data_loaded)
            .await?;
    }
    Ok(Json(ApiResponse::success(
        SetupCompleteResponse {
            setup_completed: true,
            changed,
        },
        "Shop setup recorded",
    )))
}
