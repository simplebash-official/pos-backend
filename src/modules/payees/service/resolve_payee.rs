use super::resolved_payee::ResolvedPayee;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::PayeeType;
use crate::modules::payees::repository;
use crate::modules::{employees, suppliers};

/// Looks a payee key up: `sup_…` in suppliers, `emp_…` in staff, `pye_…` in stored payees.
pub(crate) async fn resolve_payee(db: &Db, key: &str) -> AppResult<ResolvedPayee> {
    let not_found = || AppError::not_found_with_code("Payee not found.", codes::PAYEE_NOT_FOUND);
    if key.starts_with(&format!("{}_", prefixes::SUPPLIER)) {
        let supplier = suppliers::service::get_supplier_by_key(db, key).await?;
        return Ok(ResolvedPayee {
            payee_type: PayeeType::Supplier,
            name: supplier.name,
        });
    }
    if key.starts_with(&format!("{}_", prefixes::EMPLOYEE)) {
        let employee = employees::service::get_employee_by_key(db, key).await?;
        return Ok(ResolvedPayee {
            payee_type: PayeeType::Staff,
            name: employee.name,
        });
    }
    if key.starts_with(&format!("{}_", prefixes::PAYEE)) {
        let payee = repository::find_by_key(db, key)
            .await?
            .ok_or_else(not_found)?;
        if !payee.active {
            return Err(AppError::conflict(
                codes::PAYEE_HIDDEN,
                "This payee is hidden. Show it again in Settings before using it.",
            ));
        }
        return Ok(ResolvedPayee {
            payee_type: payee.payee_type,
            name: payee.name,
        });
    }
    Err(not_found())
}
