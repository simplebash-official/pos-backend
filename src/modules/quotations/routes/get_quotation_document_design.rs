use super::cache_key_for::cache_key_for;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::settings::DocumentDesignStatus;

use crate::modules::documents;
use crate::modules::quotations::service;
use axum::Json;

use axum::extract::{Path, Query, State};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct DesignStatusParams {
    template: Option<String>,
}

#[utoipa::path(get, path = "/{id}/document/design", tag = modules::QUOTATIONS,
    params(
        ("id" = String, Path, description = "Quotation ID or prefixed key"),
        ("template" = Option<String>, Query, description = "A design family for this print only"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the stored PDF was printed with versus what would print now", body = ApiResponse<DocumentDesignStatus>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Quotation not found", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available", body = ErrorResponse),
    )
)]
pub(super) async fn get_quotation_document_design(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<DesignStatusParams>,
) -> AppResult<Json<ApiResponse<DocumentDesignStatus>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let (quotation, _shop) = service::get_quotation_for_print(&state.db, &id).await?;
    let status = documents::service::print_design_status(
        &state.db,
        &state.document_server,
        "quotation",
        params.template.as_deref(),
        &quotation.key,
        cache_key_for(&quotation),
    )
    .await?;
    Ok(Json(ApiResponse::success(
        status,
        "Document design status retrieved successfully",
    )))
}
