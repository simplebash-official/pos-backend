// HTTP surface of the shop's custom design packages. Cloud (multi-tenant)
// only: a desktop's own backend has no packages to offer, so the routes answer
// 404 there. Authenticated by any signed-in shop token — web users and the
// desktop sync agent's device token alike — and scoped to that token's shop by
// `X-Tenant-Key` on the document-server call.

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        config::TenantMode,
        constants::modules,
        error::{AppError, AppResult},
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_packages))
        .routes(routes!(package_archive))
}

// ============================================================================
// Helpers
// ============================================================================

fn require_cloud(state: &AppState) -> AppResult<()> {
    if state.config.tenant_mode == TenantMode::Multi {
        Ok(())
    } else {
        Err(AppError::not_found("Not found"))
    }
}

// ============================================================================
// Packages
// ============================================================================

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DesignPackage {
    /// The immutable template id (`doc_temp_...` / `lbl_temp_...`) of this version.
    pub name: String,
    pub family: String,
    pub version: i64,
    pub kind: String,
    pub title: String,
    /// `document` or `label`.
    pub r#type: String,
    /// SHA-256 (hex) of the archive.
    pub sha256: String,
    /// Oldest desktop app version that can use it, when it has a floor.
    pub min_app_version: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DesignPackagesResponse {
    pub packages: Vec<DesignPackage>,
}

/// The latest published version of every custom (private) design the shop's
/// family grants cover. Bundled designs are not listed: every install has them.
#[utoipa::path(
    get,
    path = "/",
    tag = modules::DOCUMENT_PACKAGES,
    responses(
        (status = 200, description = "The shop's custom design packages", body = ApiResponse<DesignPackagesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Not a multi-tenant deployment", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
async fn list_packages(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<DesignPackagesResponse>>> {
    require_cloud(&state)?;
    let packages = state
        .document_server
        .list_templates()
        .await?
        .iter()
        .filter(|template| template.scope == "private")
        .map(|template| DesignPackage {
            name: template.name.clone(),
            family: template.family.clone(),
            version: template.version,
            kind: template.kind.clone(),
            title: template.title.clone(),
            r#type: template.r#type.clone(),
            sha256: template.sha256.clone(),
            min_app_version: template.min_app_version.clone(),
        })
        .collect();
    Ok(Json(ApiResponse::success(
        DesignPackagesResponse { packages },
        "Design packages retrieved successfully",
    )))
}

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
async fn package_archive(
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
