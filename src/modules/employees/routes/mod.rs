// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there. Unlike `suppliers` (blanket
// `AdminUser`), every route here is gated by a permission
// (`EMPLOYEES_READ`/`EMPLOYEES_WRITE`) — Staff genuinely needs read access
// to pick an employee when assigning a repair/print job, while only
// Admin/Manager may create/edit a profile or its commission split. There's
// no placeholder `GET /` status route — the collection root is itself the
// real "list employees" endpoint from day one, matching `suppliers`/`users`.

mod create_employee;
mod delete_employee;
mod delete_employees_batch;
mod get_employee;
mod list_employees;
mod parse_object_id;
mod router;
mod update_employee;

pub use router::router;
