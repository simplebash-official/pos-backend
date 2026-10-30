use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `DELETE /employees/batch`. Ids that fail to delete (invalid
/// `ObjectId`, not found, or blocked by the `EMPLOYEE_HAS_LOGIN` guard) are
/// silently skipped rather than failing the whole request — see
/// `service::delete_employees`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeleteEmployeesRequest {
    pub ids: Vec<String>,
}
