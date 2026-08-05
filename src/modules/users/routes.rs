// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every write route additionally calls
// `user.require_permission(permissions::USERS_MANAGE)` as the first line of
// the handler body — see `core::middleware::auth::CurrentUser` for why this
// is a method call rather than a dedicated extractor type. Unlike most
// other modules, there's no placeholder `GET /` status route here — the
// collection root (`GET /`, nested at `/api/users`) is itself the real
// "list users" endpoint from day one.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use mongodb::bson::oid::ObjectId;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions},
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
        utils::parse_object_id as parse_mongo_id,
    },
    domain::users::{CreateUserRequest, UpdateUserRequest, User, UserListQuery, UsersResponse},
    modules::users::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Users
        .routes(routes!(list_users, create_user))
        .routes(routes!(get_user, update_user, delete_user))
}

// ============================================================================
// Helpers
// ============================================================================

/// Parses a `Path<String>` id param — this is the only place in the file
/// that turns a raw path segment into an `ObjectId`; every handler that
/// needs one calls this before delegating to `service::*`.
fn parse_object_id(id: &str) -> AppResult<ObjectId> {
    parse_mongo_id(id, "User")
}

// ============================================================================
// Users
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::USERS, params(UserListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List users", body = ApiResponse<UsersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage permission", body = ErrorResponse),
    )
)]
async fn list_users(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<UserListQuery>,
) -> AppResult<Json<ApiResponse<UsersResponse>>> {
    user.require_permission(permissions::USERS_MANAGE)?;
    let response = service::list_users(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Users retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/", tag = modules::USERS, request_body = CreateUserRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "User created", body = ApiResponse<User>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage permission", body = ErrorResponse),
    )
)]
async fn create_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<User>>)> {
    user.require_permission(permissions::USERS_MANAGE)?;
    let created = service::create_user(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(created, "User created successfully")),
    ))
}

#[utoipa::path(get, path = "/{id}", tag = modules::USERS,
    params(("id" = String, Path, description = "User id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a user", body = ApiResponse<User>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage permission", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
    )
)]
async fn get_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<User>>> {
    user.require_permission(permissions::USERS_MANAGE)?;
    let object_id = parse_object_id(&id)?;
    let found = service::get_user(&state.db, object_id).await?;

    Ok(Json(ApiResponse::success(
        found,
        "User retrieved successfully",
    )))
}

#[utoipa::path(patch, path = "/{id}", tag = modules::USERS,
    params(("id" = String, Path, description = "User id")),
    request_body = UpdateUserRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "User updated", body = ApiResponse<User>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage permission", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
    )
)]
async fn update_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> AppResult<Json<ApiResponse<User>>> {
    user.require_permission(permissions::USERS_MANAGE)?;
    let object_id = parse_object_id(&id)?;
    let updated = service::update_user(&state.db, object_id, body).await?;

    Ok(Json(ApiResponse::success(
        updated,
        "User updated successfully",
    )))
}

#[utoipa::path(delete, path = "/{id}", tag = modules::USERS,
    params(("id" = String, Path, description = "User id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "User deleted", body = ApiResponse<User>),
        (status = 400, description = "Cannot delete your own account", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage permission", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
    )
)]
async fn delete_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<User>>> {
    user.require_permission(permissions::USERS_MANAGE)?;
    let object_id = parse_object_id(&id)?;
    let deleted = service::delete_user(&state.db, object_id, &user.user_id).await?;

    Ok(Json(ApiResponse::success(
        deleted,
        "User deleted successfully",
    )))
}
