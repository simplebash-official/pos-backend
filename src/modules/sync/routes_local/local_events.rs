use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::ErrorResponse;

use crate::modules::sync::{live, state};

use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::sse::Sse;

#[utoipa::path(get, path = "/local/events", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, content_type = "text/event-stream",
         description = "Server-sent events: `outbox` once on connect and after every successful local write \
                        (coalesced), so the sync agent uploads at once instead of on its next timer.",
         body = String),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
        (status = 404, description = "Not a desktop (SQLite) install", body = ErrorResponse),
    )
)]
pub(super) async fn local_events(
    _agent: SyncAgent,
    State(app): State<AppState>,
) -> AppResult<impl IntoResponse> {
    state::pool_of(&app.db)?;
    Ok(Sse::new(live::local_events()).keep_alive(live::keep_alive()))
}
