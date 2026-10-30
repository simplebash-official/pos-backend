use super::require_cloud::require_cloud;
use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The zip archive of one custom design the shop may use, as the
/// document-server stores it (verify it against the listed `sha256`).
#[utoipa::path(
    get,
    path = "/{name}/archive",
    tag = modules::DOCUMENT_PACKAGES,
    params(("name" = String, Path, description = "The package's template name, as listed")),
    responses(
        (status = 200, description = "The package archive", content_type = "application/zip", body = Vec<u8>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Not a package this shop may use, or not a multi-tenant deployment", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub(super) async fn package_archive(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    require_cloud(&state)?;
    let bytes = state.document_server.package_archive(&name).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/zip")],
        Body::from(bytes),
    )
        .into_response())
}
