use super::create_supplier;
use super::delete_supplier;
use super::delete_suppliers_batch;
use super::get_supplier;
use super::get_supplier_categories;
use super::get_supplier_stats;
use super::list_suppliers;
use super::replace_supplier;
use super::update_supplier;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Suppliers
        .routes(routes!(
            list_suppliers::list_suppliers,
            create_supplier::create_supplier
        ))
        .routes(routes!(
            get_supplier::get_supplier,
            replace_supplier::replace_supplier,
            update_supplier::update_supplier,
            delete_supplier::delete_supplier
        ))
        .routes(routes!(delete_suppliers_batch::delete_suppliers_batch))
        .routes(routes!(get_supplier_categories::get_supplier_categories))
        .routes(routes!(get_supplier_stats::get_supplier_stats))
}
