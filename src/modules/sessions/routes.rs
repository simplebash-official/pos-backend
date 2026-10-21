// HTTP layer for register sessions, cash movements, drawer ledger, and Z-reports.

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::AppResult,
        middleware::{auth::CurrentUser, sync_headers::DeviceId},
        response::{ApiResponse, ErrorResponse},
    },
    domain::{
        sessions::{
            CashMovement, CashMovementListQuery, CashMovementListResponse, CloseSessionRequest,
            ConvertVarianceRequest, CreateCashMovementRequest, DrawerState, OpenSessionRequest,
            RegisterSession, ReopenSessionRequest, SessionListQuery, SessionListResponse,
            SessionStatus, SubmitBlindCountRequest, ZReportResponse,
        },
        settings::{DocumentDesignChoice, DocumentDesignStatus},
    },
    modules::{
        documents,
        sessions::{repository::drawer, service},
        settings,
    },
};

#[derive(Debug, Deserialize)]
pub struct CurrentSessionQuery {
    pub terminal_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZReportDocumentQueryParams {
    template: Option<String>,
    design: DocumentDesignChoice,
}

#[derive(Debug, Deserialize)]
struct ZReportDesignStatusParams {
    template: Option<String>,
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(open_session))
        .routes(routes!(get_current_session))
        .routes(routes!(get_drawer_state))
        .routes(routes!(list_sessions))
        .routes(routes!(create_cash_movement))
        .routes(routes!(list_cash_movements))
        .routes(routes!(get_cash_movement))
        .routes(routes!(submit_blind_count))
        .routes(routes!(close_session))
        .routes(routes!(reopen_session))
        .routes(routes!(convert_variance))
        .routes(routes!(get_z_report))
        .routes(routes!(get_z_report_document))
        .routes(routes!(get_z_report_document_design))
        .routes(routes!(get_session))
}

#[utoipa::path(
    post,
    path = "/open",
    request_body = OpenSessionRequest,
    responses(
        (status = 201, description = "Register session opened successfully", body = ApiResponse<RegisterSession>),
        (status = 400, description = "Validation failed", body = ErrorResponse),
        (status = 409, description = "Active session already exists", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn open_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<OpenSessionRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<RegisterSession>>)> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let session = service::open_session(
        &state.db,
        current_user.user_id,
        current_user.name.unwrap_or_default(),
        body,
        device_id.0,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(ApiResponse::success(session, "Register session opened successfully"))))
}

#[utoipa::path(
    get,
    path = "/current",
    params(
        ("terminal_id" = Option<String>, Query, description = "Terminal ID to check active session for")
    ),
    responses(
        (status = 200, description = "Current active session for terminal or cashier", body = ApiResponse<Option<RegisterSession>>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn get_current_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Query(query): Query<CurrentSessionQuery>,
) -> AppResult<Json<ApiResponse<Option<RegisterSession>>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;

    let terminal_id = query.terminal_id.as_deref().or(device_id.0.as_deref());
    let session = service::resolve_active_session(&state.db, terminal_id, &current_user.user_id)
        .await?
        .map(|s| s.into_register_session());

    Ok(Json(ApiResponse::success(session, "Active register session retrieved successfully")))
}

#[utoipa::path(
    get,
    path = "/{key}",
    params(
        ("key" = String, Path, description = "Unique register session key or number")
    ),
    responses(
        (status = 200, description = "Register session details", body = ApiResponse<RegisterSession>),
        (status = 404, description = "Session not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn get_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let session = service::get_session(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(session, "Register session details retrieved successfully")))
}

#[utoipa::path(
    get,
    path = "",
    params(SessionListQuery),
    responses(
        (status = 200, description = "List of register sessions", body = ApiResponse<SessionListResponse>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn list_sessions(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<SessionListQuery>,
) -> AppResult<Json<ApiResponse<SessionListResponse>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let response = service::list_sessions(&state.db, query).await?;
    Ok(Json(ApiResponse::success(response, "Register sessions retrieved successfully")))
}

#[utoipa::path(
    post,
    path = "/{key}/blind-count",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = SubmitBlindCountRequest,
    responses(
        (status = 200, description = "Blind count recorded", body = ApiResponse<RegisterSession>),
        (status = 404, description = "Session not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn submit_blind_count(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<SubmitBlindCountRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let session = service::submit_blind_count(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(session, "Blind count recorded successfully")))
}

#[utoipa::path(
    post,
    path = "/{key}/close",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = CloseSessionRequest,
    responses(
        (status = 200, description = "Register session closed", body = ApiResponse<RegisterSession>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn close_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<CloseSessionRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let session = service::close_session(
        &state.db,
        &key,
        current_user.user_id,
        current_user.name.unwrap_or_default(),
        body,
    )
    .await?;

    Ok(Json(ApiResponse::success(session, "Register session closed successfully")))
}

#[utoipa::path(
    post,
    path = "/{key}/reopen",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = ReopenSessionRequest,
    responses(
        (status = 200, description = "Register session reopened", body = ApiResponse<RegisterSession>),
        (status = 403, description = "Forbidden: requires manager/admin", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn reopen_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<ReopenSessionRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let session = service::reopen_session(
        &state.db,
        &key,
        current_user.user_id,
        current_user.name.unwrap_or_default(),
        current_user.role,
        body,
    )
    .await?;

    Ok(Json(ApiResponse::success(session, "Register session reopened successfully")))
}

#[utoipa::path(
    post,
    path = "/{key}/convert-variance",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = ConvertVarianceRequest,
    responses(
        (status = 201, description = "Variance converted to late owner withdrawal", body = ApiResponse<CashMovement>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn convert_variance(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Path(key): Path<String>,
    Json(body): Json<ConvertVarianceRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let movement = service::convert_variance(
        &state.db,
        &key,
        current_user.user_id,
        current_user.name.unwrap_or_default(),
        body,
        device_id.0,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(ApiResponse::success(movement, "Variance converted to late owner withdrawal"))))
}

#[utoipa::path(
    get,
    path = "/{key}/z-report",
    params(
        ("key" = String, Path, description = "Register session key or number")
    ),
    responses(
        (status = 200, description = "Z-Report end of session summary", body = ApiResponse<ZReportResponse>),
        (status = 404, description = "Session not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn get_z_report(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<ZReportResponse>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let report = service::generate_z_report(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(report, "Z-Report generated successfully")))
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
async fn get_z_report_document(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Query(params): Query<ZReportDocumentQueryParams>,
) -> AppResult<Response> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let report = service::generate_z_report(&state.db, &key).await?;
    let shop = settings::service::get_shop_profile(&state.db, current_user.tenant_id.as_deref()).await?;
    let shop_val = serde_json::to_value(&shop).unwrap_or_default();
    let data = crate::modules::billing::service::print_payload::build_z_report_data(&report, &shop_val);

    let pdf_bytes = if report.session.status == SessionStatus::Closed {
        documents::service::print(
            &state.db,
            &state.document_server,
            &state.config.generated_documents_dir,
            "z-report",
            params.template.as_deref(),
            params.design,
            &report.session.key,
            "z-report",
            data,
        )
        .await?
    } else {
        documents::service::render_live(
            &state.db,
            &state.document_server,
            "z-report",
            params.template.as_deref(),
            data,
        )
        .await?
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf_bytes),
    )
        .into_response())
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
async fn get_z_report_document_design(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Query(params): Query<ZReportDesignStatusParams>,
) -> AppResult<Json<ApiResponse<DocumentDesignStatus>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let report = service::generate_z_report(&state.db, &key).await?;
    let status = documents::service::print_design_status(
        &state.db,
        &state.document_server,
        "z-report",
        params.template.as_deref(),
        &report.session.key,
        "z-report",
    )
    .await?;
    Ok(Json(ApiResponse::success(
        status,
        "Z-Report document design status retrieved successfully",
    )))
}

#[utoipa::path(
    post,
    path = "/movements",
    request_body = CreateCashMovementRequest,
    responses(
        (status = 201, description = "Cash movement recorded", body = ApiResponse<CashMovement>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
async fn create_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<CreateCashMovementRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
    let movement = service::record_movement(
        &state.db,
        current_user.user_id,
        current_user.name.unwrap_or_default(),
        body,
        device_id.0,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(ApiResponse::success(movement, "Cash movement recorded successfully"))))
}

#[utoipa::path(
    get,
    path = "/movements",
    params(CashMovementListQuery),
    responses(
        (status = 200, description = "List of cash movements", body = ApiResponse<CashMovementListResponse>)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
async fn list_cash_movements(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<CashMovementListQuery>,
) -> AppResult<Json<ApiResponse<CashMovementListResponse>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let response = service::list_movements(&state.db, query).await?;
    Ok(Json(ApiResponse::success(response, "Cash movements retrieved successfully")))
}

#[utoipa::path(
    get,
    path = "/movements/{key}",
    params(
        ("key" = String, Path, description = "Cash movement key or number")
    ),
    responses(
        (status = 200, description = "Cash movement details", body = ApiResponse<CashMovement>),
        (status = 404, description = "Movement not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
async fn get_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<CashMovement>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let movement = service::get_movement(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(movement, "Cash movement details retrieved successfully")))
}

#[utoipa::path(
    get,
    path = "/drawer-state/{terminal_id}",
    params(
        ("terminal_id" = String, Path, description = "Terminal identifier")
    ),
    responses(
        (status = 200, description = "Current drawer ledger state", body = ApiResponse<Option<DrawerState>>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
async fn get_drawer_state(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(terminal_id): Path<String>,
) -> AppResult<Json<ApiResponse<Option<DrawerState>>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let drawer_state = drawer::get_drawer_state(&state.db, &terminal_id).await?;
    Ok(Json(ApiResponse::success(drawer_state.map(|s| s.into_drawer_state()), "Drawer state retrieved successfully")))
}
