use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::ErrorResponse;

use crate::domain::settings::DocumentDesignChoice;

use crate::modules::sessions::service;
use crate::modules::{documents, settings};

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ZReportDocumentQueryParams {
    template: Option<String>,
    design: DocumentDesignChoice,
}

#[utoipa::path(
    get,
    path = "/{key}/document",
    params(
        ("key" = String, Path, description = "Register session key or number"),
        ("template" = Option<String>, Query, description = "A design family for this print only; defaults to the shop's choice for z-reports"),
        ("design" = DocumentDesignChoice, Query, description = "'original' = the stored PDF as first printed, 'current' = the design in force now"),
    ),
    responses(
        (status = 200, description = "Z-Report PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Session not found", body = ErrorResponse),
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_z_report_document(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Query(params): Query<ZReportDocumentQueryParams>,
) -> AppResult<Response> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let report = service::generate_z_report(&state.db, &key).await?;
    let shop =
        settings::service::get_shop_profile(&state.db, current_user.tenant_id.as_deref()).await?;
    let shop_val = serde_json::to_value(&shop).unwrap_or_default();
    let data =
        crate::modules::billing::service::print_payload::build_z_report_data(&report, &shop_val);

    let legacy_cache_key = format!("z-report-v{}", report.session.version);
    let pdf_bytes = documents::service::print(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        "z-report",
        params.template.as_deref(),
        params.design,
        &report.session.key,
        &legacy_cache_key,
        data,
    )
    .await?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf_bytes),
    )
        .into_response())
}
