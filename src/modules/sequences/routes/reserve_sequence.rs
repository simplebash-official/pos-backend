use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sequences::{ReserveSequenceRequest, SequenceReservationResponse};
use crate::modules::sequences::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(post, path = "/{name}/reserve", tag = modules::SEQUENCES,
    params(("name" = String, Path, description = "Sequence name (invoice, repair, printJob, purchase)")),
    request_body = ReserveSequenceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sequence block reserved", body = ApiResponse<SequenceReservationResponse>),
        (status = 400, description = "Unknown sequence name", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn reserve_sequence(
    _current_user: CurrentUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ReserveSequenceRequest>,
) -> AppResult<Json<ApiResponse<SequenceReservationResponse>>> {
    let response = service::reserve_sequence(&state.db, name, body).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Sequence block reserved successfully",
    )))
}
