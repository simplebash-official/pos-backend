use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::employees::repository;

/// Cross-module entry point `users::service` calls after linking/unlinking
/// an `employeeKey` on a login (create, update, or delete) — see
/// `repository::touch_employee_by_key` for why this is necessary at all.
/// Silently does nothing if `key` no longer resolves to a live employee
/// (e.g. the employee was deleted between the two operations); that's not
/// this caller's problem to raise.
pub(crate) async fn touch_by_key(db: &Db, key: &str) -> AppResult<()> {
    repository::touch_employee_by_key(db, key).await
}
