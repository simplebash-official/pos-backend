use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::constants::permissions as perm;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::notifications::{
    CreateNotificationRequest, MarkAllReadRequest, MarkReadRequest, MarkReadResponse, Notification,
    NotificationListQuery, NotificationPage, NotificationSummary,
};
use crate::modules::notifications::{live, service};
use crate::modules::sync::live as sync_live;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::response::sse::Sse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

// ----- History --------------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/",
    params(NotificationListQuery),
    responses(
        (status = 200, description = "A page of the caller's notifications, newest first", body = ApiResponse<NotificationPage>),
        (status = 400, description = "Bad filter, page size or page link", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = modules::NOTIFICATIONS
)]
pub(super) async fn list_notifications(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<NotificationListQuery>,
) -> AppResult<Json<ApiResponse<NotificationPage>>> {
    current_user.require_permission(perm::NOTIFICATIONS_VIEW)?;
    let page = service::list(&state.db, &current_user, query).await?;
    Ok(Json(ApiResponse::success(
        page,
        "Notifications retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/summary",
    responses(
        (status = 200, description = "Unread counts and the newest notifications, for the bell", body = ApiResponse<NotificationSummary>)
    ),
    security(("bearerAuth" = [])),
    tag = modules::NOTIFICATIONS
)]
pub(super) async fn notification_summary(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<NotificationSummary>>> {
    current_user.require_permission(perm::NOTIFICATIONS_VIEW)?;
    let summary = service::summary(&state.db, &current_user).await?;
    Ok(Json(ApiResponse::success(
        summary,
        "Notification summary retrieved successfully",
    )))
}

// ----- Read state -----------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/read",
    request_body = MarkReadRequest,
    responses(
        (status = 200, description = "Marked read for the caller (unknown or hidden keys are ignored)", body = ApiResponse<MarkReadResponse>),
        (status = 400, description = "No keys, or more than 100", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = modules::NOTIFICATIONS
)]
pub(super) async fn mark_notifications_read(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<MarkReadRequest>,
) -> AppResult<Json<ApiResponse<MarkReadResponse>>> {
    current_user.require_permission(perm::NOTIFICATIONS_VIEW)?;
    let result = service::mark_read(&state.db, &current_user, body.keys).await?;
    Ok(Json(ApiResponse::success(result, "Marked as read")))
}

#[utoipa::path(
    post,
    path = "/read-all",
    request_body = MarkAllReadRequest,
    responses(
        (status = 200, description = "Every unread notification matching the filter marked read for the caller", body = ApiResponse<MarkReadResponse>),
        (status = 400, description = "Bad filter", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = modules::NOTIFICATIONS
)]
pub(super) async fn mark_all_notifications_read(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<MarkAllReadRequest>,
) -> AppResult<Json<ApiResponse<MarkReadResponse>>> {
    current_user.require_permission(perm::NOTIFICATIONS_VIEW)?;
    let result = service::mark_all_read(&state.db, &current_user, body).await?;
    Ok(Json(ApiResponse::success(result, "Marked as read")))
}

// ----- Raising --------------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/",
    request_body = CreateNotificationRequest,
    responses(
        (status = 201, description = "Raised (or, for a used dedupe key, the one already raised)", body = ApiResponse<Notification>),
        (status = 400, description = "Missing or too long text, or a link outside the app", body = ErrorResponse),
        (status = 403, description = "An audience the caller may not address", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = modules::NOTIFICATIONS
)]
pub(super) async fn create_notification(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreateNotificationRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Notification>>)> {
    current_user.require_permission(perm::NOTIFICATIONS_VIEW)?;
    let notification = service::create_from_app(&state.db, &current_user, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(notification, "Notification raised")),
    ))
}

// ----- Live -----------------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/events",
    responses(
        (status = 200, content_type = "text/event-stream",
         description = "Server-sent events: `hello` on connect, then `changed` whenever the shop's \
                        notifications or read marks change, and a `ping` comment every 15 s. Carries \
                        no data: reload the summary or history.",
         body = String),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = modules::NOTIFICATIONS
)]
pub(super) async fn notification_events(current_user: CurrentUser) -> AppResult<impl IntoResponse> {
    current_user.require_permission(perm::NOTIFICATIONS_VIEW)?;
    let events = live::events()
        .ok_or_else(|| AppError::unauthorized("This token is not bound to a shop"))?;
    Ok((
        // nginx would otherwise buffer the stream until it closes.
        [(
            HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        )],
        Sse::new(events).keep_alive(sync_live::keep_alive()),
    ))
}

/// Routes of the notifications module, mounted at `/api/notifications`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // History
        .routes(routes!(list_notifications, create_notification))
        .routes(routes!(notification_summary))
        // Read state
        .routes(routes!(mark_notifications_read))
        .routes(routes!(mark_all_notifications_read))
        // Live
        .routes(routes!(notification_events))
}
