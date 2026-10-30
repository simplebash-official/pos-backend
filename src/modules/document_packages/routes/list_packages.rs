use super::design_package::DesignPackage;
use super::design_packages_response::DesignPackagesResponse;
use super::require_cloud::require_cloud;
use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use axum::Json;
use axum::extract::State;

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
pub(super) async fn list_packages(
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
