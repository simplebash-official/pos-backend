use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::ApiResponse;
use crate::core::response::ErrorResponse;
use crate::domain::sessions::ActiveSessionsSummaryResponse;
use crate::domain::sessions::CashMovement;
use crate::domain::sessions::CashMovementListQuery;
use crate::domain::sessions::CashMovementListResponse;
use crate::domain::sessions::CloseSessionRequest;
use crate::domain::sessions::ConvertVarianceRequest;
use crate::domain::sessions::CreateCashMovementRequest;
use crate::domain::sessions::DrawerState;
use crate::domain::sessions::OpenSessionRequest;
use crate::domain::sessions::RegisterSession;
use crate::domain::sessions::ReopenSessionRequest;
use crate::domain::sessions::ReverseCashMovementRequest;
use crate::domain::sessions::SessionListQuery;
use crate::domain::sessions::SessionListResponse;
use crate::domain::sessions::SubmitBlindCountRequest;
use crate::domain::sessions::ZReportResponse;
use crate::domain::settings::DocumentDesignChoice;
use crate::domain::settings::DocumentDesignStatus;
use crate::domain::users::Role;
use crate::modules::sessions::repository::drawer;
use crate::modules::sessions::service;
use crate::modules::{documents, settings};
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(
    post,
    path = "/{key}/cancel-close",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    responses(
        (status = 200, description = "Closing cancelled, session back in use", body = ApiResponse<RegisterSession>),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 409, description = "Session already closed or version conflict", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn cancel_close(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let session = service::cancel_close(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        session,
        "Closing cancelled successfully",
    )))
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
        (status = 400, description = "Validation failed", body = ErrorResponse),
        (status = 409, description = "Session already closed or version conflict", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn close_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<CloseSessionRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let cashier_name = current_user.resolve_name(&state.db).await?;
    let session =
        service::close_session(&state.db, &key, current_user.user_id, cashier_name, body).await?;

    Ok(Json(ApiResponse::success(
        session,
        "Register session closed successfully",
    )))
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
pub(super) async fn convert_variance(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Path(key): Path<String>,
    Json(body): Json<ConvertVarianceRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let cashier_name = current_user.resolve_name(&state.db).await?;
    let movement = service::convert_variance(
        &state.db,
        &key,
        current_user.user_id,
        cashier_name,
        body,
        device_id.0,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            movement,
            "Variance converted to late owner withdrawal",
        )),
    ))
}

#[utoipa::path(
    post,
    path = "/movements",
    request_body = CreateCashMovementRequest,
    responses(
        (status = 201, description = "Cash movement recorded", body = ApiResponse<CashMovement>),
        (status = 400, description = "Validation failed", body = ErrorResponse),
        (status = 409, description = "Session closing lock or drawer limit conflict", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
pub(super) async fn create_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<CreateCashMovementRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
    if service::requires_managed_permission(body.movement_type) {
        current_user.require_permission(perm::CASH_MOVEMENTS_CREATE_MANAGED)?;
    }
    let cashier_name = current_user.resolve_name(&state.db).await?;
    let movement = service::record_movement(
        &state.db,
        current_user.user_id,
        cashier_name,
        body,
        device_id.0,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            movement,
            "Cash movement recorded successfully",
        )),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CurrentSessionQuery {
    pub terminal_id: Option<String>,
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
pub(super) async fn get_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<CashMovement>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let movement = service::get_movement(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        movement,
        "Cash movement details retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/active-summary",
    responses(
        (status = 200, description = "Active register sessions across open/closing/reopened and summed drawer balance", body = ApiResponse<ActiveSessionsSummaryResponse>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_active_sessions_summary(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<ActiveSessionsSummaryResponse>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let summary = service::get_active_sessions_summary(&state.db).await?;
    Ok(Json(ApiResponse::success(
        summary,
        "Active sessions summary retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/current",
    params(
        ("terminal_id" = Option<String>, Query, description = "Terminal ID to check active session for")
    ),
    responses(
        (status = 200, description = "Current active session for user (Personal) or shop (SharedShop); filtered to opening terminal for Staff when allowStaffMultiDeviceSessions is false in Personal mode", body = ApiResponse<Option<RegisterSession>>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_current_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Query(query): Query<CurrentSessionQuery>,
) -> AppResult<Json<ApiResponse<Option<RegisterSession>>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;

    let terminal_id = query.terminal_id.as_deref().or(device_id.0.as_deref());
    let policy = service::current_shift_policy(&state.db).await?;
    let session = service::resolve_active_session(&state.db, &current_user.user_id)
        .await?
        // In Personal mode with `allowStaffMultiDeviceSessions = false`, Staff work from one
        // machine: their shift is not shown on any other terminal. When multi-device is enabled
        // or the shop uses SharedShop mode, Staff see their active shift from any terminal.
        .filter(|s| {
            policy.allows_staff_multi_device()
                || current_user.role != Some(Role::Staff)
                || Some(s.terminal_id.as_str()) == terminal_id
        })
        .map(|s| s.into_register_session());

    Ok(Json(ApiResponse::success(
        session,
        "Active register session retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/drawer-state",
    responses(
        (status = 200, description = "Current drawer balance (signed-in user's drawer in Personal mode, or 'shop' in SharedShop mode)", body = ApiResponse<Option<DrawerState>>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_drawer_state(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<Option<DrawerState>>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let drawer_key = service::active_drawer_key(&state.db, None, &current_user.user_id).await?;
    let drawer_state = drawer::get_drawer_state(&state.db, &drawer_key).await?;
    Ok(Json(ApiResponse::success(
        drawer_state.map(|s| s.into_drawer_state()),
        "Drawer state retrieved successfully",
    )))
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
pub(super) async fn get_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let session = service::get_session(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        session,
        "Register session details retrieved successfully",
    )))
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
pub(super) async fn get_z_report(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<ZReportResponse>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let report = service::generate_z_report(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Z-Report generated successfully",
    )))
}

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
    let shop = settings::service::get_shop_profile(&state.db).await?;
    let shop_val = serde_json::to_value(&shop)?;
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
pub(super) async fn list_cash_movements(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<CashMovementListQuery>,
) -> AppResult<Json<ApiResponse<CashMovementListResponse>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let response = service::list_movements(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Cash movements retrieved successfully",
    )))
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
pub(super) async fn list_sessions(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<SessionListQuery>,
) -> AppResult<Json<ApiResponse<SessionListResponse>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let response = service::list_sessions(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Register sessions retrieved successfully",
    )))
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
pub(super) async fn open_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<OpenSessionRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<RegisterSession>>)> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let cashier_name = current_user.resolve_name(&state.db).await?;
    let session = service::open_session(
        &state.db,
        current_user.user_id,
        cashier_name,
        body,
        device_id.0,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            session,
            "Register session opened successfully",
        )),
    ))
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
pub(super) async fn reopen_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<ReopenSessionRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let cashier_name = current_user.resolve_name(&state.db).await?;
    let session = service::reopen_session(
        &state.db,
        &key,
        current_user.user_id,
        cashier_name,
        current_user.role,
        body,
    )
    .await?;

    Ok(Json(ApiResponse::success(
        session,
        "Register session reopened successfully",
    )))
}

#[utoipa::path(
    post,
    path = "/movements/{key}/reverse",
    params(("key" = String, Path, description = "Key of the movement to reverse")),
    request_body = ReverseCashMovementRequest,
    responses(
        (status = 201, description = "The reversing entry", body = ApiResponse<CashMovement>),
        (status = 403, description = "Only a manager can reverse this now", body = ErrorResponse),
        (status = 409, description = "Already reversed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
pub(super) async fn reverse_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Path(key): Path<String>,
    Json(body): Json<ReverseCashMovementRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
    let can_reverse = current_user.has_permission(perm::CASH_MOVEMENTS_REVERSE);
    let name = current_user.resolve_name(&state.db).await?;
    let movement = service::reverse_movement(
        &state.db,
        &key,
        current_user.user_id,
        name,
        can_reverse,
        body,
        device_id.0,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            movement,
            "Cash movement reversed successfully",
        )),
    ))
}

/// Routes of the sessions module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(open_session))
        .routes(routes!(get_active_sessions_summary))
        .routes(routes!(get_current_session))
        .routes(routes!(get_drawer_state))
        .routes(routes!(list_sessions))
        .routes(routes!(create_cash_movement))
        .routes(routes!(reverse_cash_movement))
        .routes(routes!(list_cash_movements))
        .routes(routes!(get_cash_movement))
        .routes(routes!(submit_blind_count))
        .routes(routes!(cancel_close))
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
    path = "/{key}/blind-count",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = SubmitBlindCountRequest,
    responses(
        (status = 200, description = "Blind count recorded", body = ApiResponse<RegisterSession>),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 409, description = "Session already closed or version conflict", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn submit_blind_count(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<SubmitBlindCountRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let cashier_name = current_user.resolve_name(&state.db).await?;
    let session =
        service::submit_blind_count(&state.db, &key, current_user.user_id, cashier_name, body)
            .await?;
    Ok(Json(ApiResponse::success(
        session,
        "Blind count recorded successfully",
    )))
}
