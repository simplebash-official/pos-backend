use super::require_cloud::require_cloud;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};

use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::ErrorResponse;
use crate::core::sync_origin::origin_for;
use crate::core::tenancy::current_tenant_id;

use crate::modules::sync::{cloud_store, live};

use axum::extract::State;
use axum::http::{HeaderName, HeaderValue};
use axum::response::IntoResponse;
use axum::response::sse::Sse;

#[utoipa::path(get, path = "/events", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, content_type = "text/event-stream",
         description = "Server-sent events: `hello` {latestSeq} on connect, then `change` {seq, resource, origin} \
                        for every change written by anyone but the caller, `resync` when the caller fell behind \
                        (pull to catch up), and a `ping` comment every 15 s. Carries no record data: read the \
                        change through `GET /pull` (devices) or the REST API (browsers).",
         body = live::LiveEvent),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn change_events(
    user: CurrentUser,
    DeviceId(header_device): DeviceId,
    State(state): State<AppState>,
) -> AppResult<impl IntoResponse> {
    require_cloud(&state)?;
    let tenant = current_tenant_id()
        .ok_or_else(|| AppError::unauthorized("This token is not bound to a shop"))?;
    // Same origin the caller's own writes are stamped with, so its echoes are
    // left out of its stream.
    let origin = origin_for(user.device_id.as_deref(), header_device.as_deref());
    // Subscribe before reading the position: a change written in between is
    // then announced rather than lost.
    let receiver = live::subscribe(&tenant);
    let latest = cloud_store::current_seq(&state.db).await?;
    let events = live::cloud_events(receiver, latest, origin);
    Ok((
        // nginx would otherwise buffer the stream until it closes.
        [(
            HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        )],
        Sse::new(events).keep_alive(live::keep_alive()),
    ))
}
