use axum::{
    Json,
    extract::{Path, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::sequences::{ReserveSequenceRequest, SequenceReservationResponse},
    modules::sequences::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(reserve_sequence))
}

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
async fn reserve_sequence(
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
