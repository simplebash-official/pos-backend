// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every route calls
// `user.require_any_permission(&[USERS_MANAGE, USERS_MANAGE_STAFF])` as the
// first line of the handler body — see `core::middleware::auth::CurrentUser`
// for why this is a method call rather than a dedicated extractor type —
// then passes the caller's role into `service::*`, which enforces the
// actual Admin/Manager management hierarchy (an Admin manages Manager and
// Staff accounts, a Manager manages Staff accounts only — see
// `modules::users::service::manageable_roles`). Unlike most other modules,
// there's no placeholder `GET /` status route here — the collection root
// (`GET /`, nested at `/api/users`) is itself the real "list users"
// endpoint from day one.

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
        error::{AppError, AppResult},
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
        utils::parse_object_id as parse_mongo_id,
    },
    domain::users::{
        CreateUserRequest, Role, UpdateUserRequest, User, UserListQuery, UsersResponse,
    },
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

/// Every handler needs the caller's role to pass into `service::*`'s
/// hierarchy checks — `require_any_permission` above it already guarantees
/// `role` is `Some` in practice (only `Role::Admin`/`Role::Manager` are ever
/// granted `USERS_MANAGE`/`USERS_MANAGE_STAFF`), but the claim is still
/// typed `Option<Role>`, so this turns a missing role into a 403 rather
/// than a panic.
fn require_caller_role(user: &CurrentUser) -> AppResult<Role> {
    user.role
        .ok_or_else(|| AppError::forbidden("Token is missing a role claim"))
}

// ============================================================================
// Users
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::USERS, params(UserListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List users — scoped to the roles the caller may manage (Admin: Manager+Staff; Manager: Staff only)", body = ApiResponse<UsersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage / users:manage:staff permission", body = ErrorResponse),
    )
)]
async fn list_users(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<UserListQuery>,
) -> AppResult<Json<ApiResponse<UsersResponse>>> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let response = service::list_users(&state.db, query, caller_role).await?;

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
        (status = 403, description = "Missing permission, or the target role is outside what the caller may create (e.g. a Manager creating a Manager/Admin)", body = ErrorResponse),
        (status = 409, description = "Email already exists, or (creating an Admin) an Admin account already exists", body = ErrorResponse),
    )
)]
async fn create_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<User>>)> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let created = service::create_user_for_caller(&state.db, body, caller_role).await?;

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
        (status = 403, description = "Missing users:manage / users:manage:staff permission", body = ErrorResponse),
        (status = 404, description = "User not found, or outside what the caller may manage", body = ErrorResponse),
    )
)]
async fn get_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<User>>> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let object_id = parse_object_id(&id)?;
    let found = service::get_user_for_caller(&state.db, object_id, caller_role).await?;

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
        (status = 403, description = "Missing permission, or the requested role change is outside what the caller may set", body = ErrorResponse),
        (status = 404, description = "User not found, or outside what the caller may manage", body = ErrorResponse),
        (status = 409, description = "Email already exists", body = ErrorResponse),
    )
)]
async fn update_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> AppResult<Json<ApiResponse<User>>> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let object_id = parse_object_id(&id)?;
    let updated = service::update_user(&state.db, object_id, body, caller_role).await?;

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
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage / users:manage:staff permission", body = ErrorResponse),
        (status = 404, description = "User not found, or outside what the caller may manage", body = ErrorResponse),
    )
)]
async fn delete_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<User>>> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let object_id = parse_object_id(&id)?;
    let deleted = service::delete_user(&state.db, object_id, caller_role).await?;

    Ok(Json(ApiResponse::success(
        deleted,
        "User deleted successfully",
    )))
}
