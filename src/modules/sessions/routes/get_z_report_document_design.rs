use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::settings::DocumentDesignStatus;

use crate::modules::documents;
use crate::modules::sessions::service;
use axum::Json;

use axum::extract::{Path, Query, State};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ZReportDesignStatusParams {
    template: Option<String>,
}

#[utoipa::path(
    get,
    path = "/{key}/document/design",
    params(
        ("key" = String, Path, description = "Register session key or number"),
        ("template" = Option<String>, Query, description = "A design family for this print only"),
    ),
    responses(
        (status = 200, description = "Z-Report design status", body = ApiResponse<DocumentDesignStatus>),
        (status = 404, description = "Session not found", body = ErrorResponse),
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_z_report_document_design(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Query(params): Query<ZReportDesignStatusParams>,
) -> AppResult<Json<ApiResponse<DocumentDesignStatus>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let report = service::generate_z_report(&state.db, &key).await?;
    let legacy_cache_key = format!("z-report-v{}", report.session.version);
    let status = documents::service::print_design_status(
        &state.db,
        &state.document_server,
        "z-report",
        params.template.as_deref(),
        &report.session.key,
        &legacy_cache_key,
    )
    .await?;
    Ok(Json(ApiResponse::success(
        status,
        "Z-Report document design status retrieved successfully",
    )))
}
