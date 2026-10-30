use super::authorize::authorize;
use super::document_selections_response::DocumentSelectionsResponse;
use super::to_selections::to_selections;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::tenants::service;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

/// The document designs a shop has chosen, so the console can tell whether a
/// design is in use before revoking it. Requires `X-Provision-Secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/document-selections",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    responses(
        (status = 200, description = "The shop's chosen designs", body = ApiResponse<DocumentSelectionsResponse>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn shop_document_selections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> AppResult<Json<ApiResponse<DocumentSelectionsResponse>>> {
    authorize(&state, &headers)?;
    let selections = service::document_selections(&state.db, tenant_id.trim()).await?;
    Ok(Json(ApiResponse::success(
        DocumentSelectionsResponse {
            selections: to_selections(selections),
        },
        "Document selections",
    )))
}
