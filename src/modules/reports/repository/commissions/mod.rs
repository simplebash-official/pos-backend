// MongoDB-only aggregation of technician & staff commission figures across
// repairs and print jobs. Grouped by `assigned_employee_id` (a real
// `modules::employees` key), never by the free-text `assigned_employee_name`
// display string — two employees sharing a name must not merge into one
// entry, and renaming an employee must not split their history across two
// buckets. Display name/role resolution against `employees` is deliberately
// NOT done here — this file is Mongo-only, per this codebase's "reach
// another module through its service, never its repository" rule — see
// `service::commissions::get_employee_commissions_report`, which calls
// `employees::service::get_employees_by_keys` after this function returns.

mod aggregate_employee_commissions;
mod get_i64_flexible;
mod unassigned_printer;
mod unassigned_technician;

pub(crate) use aggregate_employee_commissions::aggregate_employee_commissions;
pub(crate) use get_i64_flexible::get_i64_flexible;
pub(crate) use unassigned_printer::UNASSIGNED_PRINTER;
pub(crate) use unassigned_technician::UNASSIGNED_TECHNICIAN;
