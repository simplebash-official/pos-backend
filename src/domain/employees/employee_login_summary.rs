use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Summary of an employee's linked login account, resolved live at read time
/// via `modules::users::service::find_user_summary_by_employee_key` — `None`
/// when the employee has no login (the common case for e.g. a repair
/// technician tracked for commission but never signing in). Never persisted
/// on the employee document itself; there is deliberately no reverse
/// `user_key` field — "does this employee have a login" is always resolved
/// live against the `users` collection, the same graceful-degradation
/// `Option` pattern `Purchase`'s live-resolved `supplier`/`product` uses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeLoginSummary {
    pub user_id: String,
    pub username: String,
    pub role: crate::domain::users::Role,
    pub is_active: bool,
}
