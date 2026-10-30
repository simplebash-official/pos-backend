use super::create_customer;
use super::delete_customer;
use super::delete_customers_batch;
use super::get_customer;
use super::get_customer_stats;
use super::get_customer_tags;
use super::list_customers;
use super::replace_customer;
use super::update_customer;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Customers
        .routes(routes!(
            list_customers::list_customers,
            create_customer::create_customer
        ))
        .routes(routes!(delete_customers_batch::delete_customers_batch))
        .routes(routes!(get_customer_tags::get_customer_tags))
        .routes(routes!(get_customer_stats::get_customer_stats))
        .routes(routes!(
            get_customer::get_customer,
            replace_customer::replace_customer,
            update_customer::update_customer,
            delete_customer::delete_customer
        ))
}
