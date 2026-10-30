use crate::app::AppState;
use crate::core::config::TenantMode;
use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::verify_bearer;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::system::{SetupSystemRequest, SetupSystemResponse};
use crate::domain::users::Role;
use crate::modules::system::service;
use crate::modules::users::service as users_service;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;

/// Execute initial system setup, bootstrap the administrator account, and conditionally seed database.
#[utoipa::path(
    post,
    path = "/setup",
    tag = modules::SYSTEM,
    request_body = SetupSystemRequest,
    responses(
        (status = 200, description = "System setup completed successfully", body = ApiResponse<SetupSystemResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 409, description = "Setup already completed", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub(super) async fn perform_setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetupSystemRequest>,
) -> AppResult<Json<ApiResponse<SetupSystemResponse>>> {
    if state.config.tenant_mode == TenantMode::Multi {
        if let Ok(verified) = verify_bearer(&headers, &state.config).await {
            if verified.role != Some(Role::Admin) {
                return Err(AppError::forbidden(
                    "Only administrators can complete store setup",
                ));
            }
            let tid = verified
                .tenant_id()
                .ok_or_else(|| AppError::unauthorized("Authenticated token has no tenant scope"))?;

            let user = match mongodb::bson::oid::ObjectId::parse_str(&verified.user_id) {
                Ok(oid) => users_service::get_user(&state.db, oid).await.ok(),
                // Identity-server token: the caller is the shop's owner, i.e. its Admin.
                Err(_) => {
                    users_service::find_user_for_identity(&state.db, verified.name.as_deref())
                        .await
                        .ok()
                        .flatten()
                }
            };

            let result =
                service::perform_tenant_setup(&state.db, &tid, body.load_sample_data, user).await?;

            return Ok(Json(ApiResponse::success(
                result,
                "Tenant setup completed successfully",
            )));
        }

        return Err(AppError::unauthorized(
            "Please log in to your store to complete onboarding setup",
        ));
    }

    let result = service::perform_setup(&state.db, &state.config, body).await?;
    Ok(Json(ApiResponse::success(
        result,
        "System setup completed successfully",
    )))
}
