use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::labels::{LabelCoverageQuery, LabelCoverageResponse};
use crate::domain::users::Role;
use crate::modules::labels::service;

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(get, path = "/coverage", tag = modules::LABELS, params(LabelCoverageQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Products with their label state: never labelled, labels needed for new stock, or labelled", body = ApiResponse<LabelCoverageResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn coverage(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LabelCoverageQuery>,
) -> AppResult<Json<ApiResponse<LabelCoverageResponse>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let mut coverage = service::jobs::coverage(&state.db, query).await?;
    // What the shop paid stays Admin-only, as on the inventory routes.
    if user.role != Some(Role::Admin) {
        for row in &mut coverage.items {
            row.product = row.product.clone().without_cost();
        }
    }
    Ok(Json(ApiResponse::success(
        coverage,
        "Label coverage retrieved successfully",
    )))
}
