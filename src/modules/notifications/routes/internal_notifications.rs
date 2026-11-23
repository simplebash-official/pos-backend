use crate::app::AppState;
use crate::core::constants::{codes, modules};
use crate::core::error::{AppError, AppResult};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::tenancy::{Tenant, with_tenant};
use crate::domain::notifications::{InternalNotificationRequest, Notification};
use crate::modules::notifications::service;
use crate::modules::tenants::repository as tenants_repository;
use crate::modules::tenants::routes::authorize_internal;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// A trusted internal service sends a notice to one shop. Requires `X-Provision-Secret`.
#[utoipa::path(
    post,
    path = "/tenants/{tenant_id}/notifications",
    tag = modules::NOTIFICATIONS,
    params(("tenant_id" = String, Path, description = "The shop's tenant id")),
    request_body = InternalNotificationRequest,
    responses(
        (status = 201, description = "Sent (or, for a used dedupe key, the one already sent)", body = ApiResponse<Notification>),
        (status = 400, description = "Missing or too long text, a link outside the app, or an audience a service may not address", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn send_shop_notification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<InternalNotificationRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Notification>>)> {
    authorize_internal(&state, &headers)?;
    let tenant_id = tenant_id.trim();
    tenants_repository::find_tenant_by_key(&state.db, tenant_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code(
                format!("No shop with id '{tenant_id}'"),
                codes::TENANT_NOT_FOUND,
            )
        })?;
    let notification = with_tenant(
        Tenant::id(tenant_id)?,
        service::create_from_service(&state.db, body),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(notification, "Notification sent")),
    ))
}

/// The internal (server-to-server) notification route, mounted under `/api/internal`.
pub fn internal_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(send_shop_notification))
}
