use super::get_receipt;
use super::list_receipts;
use super::record_receipt;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Mounted at `/purchase-receipts`.
pub fn receipts_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            list_receipts::list_receipts,
            record_receipt::record_receipt
        ))
        .routes(routes!(get_receipt::get_receipt))
}
