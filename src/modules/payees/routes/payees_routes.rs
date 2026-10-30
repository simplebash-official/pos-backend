use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::cash_categories::{
    CreatePayeeRequest, Payee, PayeeListQuery, UpdatePayeeRequest,
};
use crate::modules::payees::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(
    post,
    path = "/",
    request_body = CreatePayeeRequest,
    responses(
        (status = 201, description = "Payee created", body = ApiResponse<Payee>),
        (status = 409, description = "Name already in the list", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Payees"
)]
pub(super) async fn create_payee(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreatePayeeRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Payee>>)> {
    current_user.require_permission(perm::PAYEES_MANAGE)?;
    let payee = service::create_payee(&state.db, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(payee, "Payee created successfully")),
    ))
}

#[utoipa::path(
    get,
    path = "/",
    params(PayeeListQuery),
    responses((status = 200, description = "Suppliers, staff and saved payees, frequent first", body = ApiResponse<Vec<Payee>>)),
    security(("bearerAuth" = [])),
    tag = "Payees"
)]
pub(super) async fn list_payees(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PayeeListQuery>,
) -> AppResult<Json<ApiResponse<Vec<Payee>>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    if query.include_hidden == Some(true) {
        current_user.require_permission(perm::PAYEES_MANAGE)?;
    }
    let payees = service::list_payees(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        payees,
        "Payees retrieved successfully",
    )))
}

/// Routes of the payees module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_payees))
        .routes(routes!(create_payee))
        .routes(routes!(update_payee))
}

#[utoipa::path(
    patch,
    path = "/{key}",
    params(("key" = String, Path, description = "Payee key (`pye_…`)")),
    request_body = UpdatePayeeRequest,
    responses(
        (status = 200, description = "Payee updated", body = ApiResponse<Payee>),
        (status = 400, description = "Suppliers and staff are changed on their own pages", body = ErrorResponse),
        (status = 404, description = "Payee not found", body = ErrorResponse),
        (status = 409, description = "Name already in the list", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Payees"
)]
pub(super) async fn update_payee(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<UpdatePayeeRequest>,
) -> AppResult<Json<ApiResponse<Payee>>> {
    current_user.require_permission(perm::PAYEES_MANAGE)?;
    let payee = service::update_payee(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        payee,
        "Payee updated successfully",
    )))
}
