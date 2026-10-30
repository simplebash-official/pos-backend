/// Internal bucket keys for jobs with no `assigned_employee_id` at all —
/// never resolved against `employees` and never treated as a real
/// employee's key by the caller.
pub(crate) const UNASSIGNED_TECHNICIAN: &str = "__unassigned_technician__";
