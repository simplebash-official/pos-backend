use super::get_staff_advance;
use super::list_staff_advances;
use super::list_supplier_balances;
use super::recover_advance;
use super::set_balance_opening;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_supplier_balances::list_supplier_balances))
        .routes(routes!(list_staff_advances::list_staff_advances))
        .routes(routes!(get_staff_advance::get_staff_advance))
        // Writes
        .routes(routes!(set_balance_opening::set_balance_opening))
        .routes(routes!(recover_advance::recover_advance))
}
