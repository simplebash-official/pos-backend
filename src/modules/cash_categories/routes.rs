// HTTP layer for drawer cash categories.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::cash_categories::{
        CashCategory, CashCategoryListQuery, CreateCashCategoryRequest, MergeCashCategoryRequest,
        SimilarCashCategoryQuery, UpdateCashCategoryRequest,
    },
    modules::cash_categories::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_categories))
        .routes(routes!(similar_categories))
        // Writes
        .routes(routes!(create_category))
        .routes(routes!(update_category))
        .routes(routes!(merge_category))
}

// ============================================================================
// Categories
// ============================================================================

#[utoipa::path(
    get,
    path = "/",
    params(CashCategoryListQuery),
    responses((status = 200, description = "Categories, best first", body = ApiResponse<Vec<CashCategory>>)),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
async fn list_categories(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<CashCategoryListQuery>,
) -> AppResult<Json<ApiResponse<Vec<CashCategory>>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    // Seeing hidden categories or the review queue is part of managing them.
    if query.include_hidden == Some(true) || query.needs_review == Some(true) {
        current_user.require_permission(perm::CASH_CATEGORIES_MANAGE)?;
    }
    let categories = service::list_categories(&state.db, query).await?;
    Ok(Json(ApiResponse::success(categories, "Categories retrieved successfully")))
}

#[utoipa::path(
    get,
    path = "/similar",
    params(SimilarCashCategoryQuery),
    responses((status = 200, description = "Existing categories with a similar name", body = ApiResponse<Vec<CashCategory>>)),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
async fn similar_categories(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<SimilarCashCategoryQuery>,
) -> AppResult<Json<ApiResponse<Vec<CashCategory>>>> {
    current_user.require_permission(perm::CASH_CATEGORIES_CREATE)?;
    let categories = service::similar_categories(&state.db, query).await?;
    Ok(Json(ApiResponse::success(categories, "Similar categories retrieved successfully")))
}

#[utoipa::path(
    post,
    path = "/",
    request_body = CreateCashCategoryRequest,
    responses(
        (status = 201, description = "Category created", body = ApiResponse<CashCategory>),
        (status = 403, description = "Not allowed for this kind of payment", body = ErrorResponse),
        (status = 409, description = "Name already used", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
async fn create_category(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreateCashCategoryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashCategory>>)> {
    current_user.require_permission(perm::CASH_CATEGORIES_CREATE)?;
    let name = current_user.resolve_name(&state.db).await;
    let can_manage = current_user.has_permission(perm::CASH_CATEGORIES_MANAGE);
    let category = service::create_category(&state.db, name, can_manage, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(category, "Category created successfully")),
    ))
}

#[utoipa::path(
    patch,
    path = "/{key}",
    params(("key" = String, Path, description = "Category key")),
    request_body = UpdateCashCategoryRequest,
    responses(
        (status = 200, description = "Category updated", body = ApiResponse<CashCategory>),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "Name already used", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
async fn update_category(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<UpdateCashCategoryRequest>,
) -> AppResult<Json<ApiResponse<CashCategory>>> {
    current_user.require_permission(perm::CASH_CATEGORIES_MANAGE)?;
    let category = service::update_category(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(category, "Category updated successfully")))
}

#[utoipa::path(
    post,
    path = "/{key}/merge",
    params(("key" = String, Path, description = "Key of the duplicate to merge away")),
    request_body = MergeCashCategoryRequest,
    responses(
        (status = 200, description = "Merged; the surviving category", body = ApiResponse<CashCategory>),
        (status = 404, description = "Category not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
async fn merge_category(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<MergeCashCategoryRequest>,
) -> AppResult<Json<ApiResponse<CashCategory>>> {
    current_user.require_permission(perm::CASH_CATEGORIES_MANAGE)?;
    let category = service::merge_category(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(category, "Categories merged successfully")))
}
