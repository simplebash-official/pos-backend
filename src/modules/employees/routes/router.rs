use super::create_employee;
use super::delete_employee;
use super::delete_employees_batch;
use super::get_employee;
use super::list_employees;
use super::update_employee;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Employees
        .routes(routes!(
            list_employees::list_employees,
            create_employee::create_employee
        ))
        .routes(routes!(
            get_employee::get_employee,
            update_employee::update_employee,
            delete_employee::delete_employee
        ))
        .routes(routes!(delete_employees_batch::delete_employees_batch))
}
