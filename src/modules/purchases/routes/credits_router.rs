use super::list_credits;
use super::record_credit;
use super::void_credit;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Mounted at `/supplier-credits`.
pub fn credits_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            list_credits::list_credits,
            record_credit::record_credit
        ))
        .routes(routes!(void_credit::void_credit))
}
