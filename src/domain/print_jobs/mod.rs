//! Pure business types for the print-jobs feature — mirrors `domain::repairs`
//! closely; the two modules differ only in their domain-specific fields
//! (job type/quantity vs device model/issue description). See
//! `domain::repairs`'s module comment for how `assigned_employee_id` is
//! resolved server-side against `modules::employees` and why `split_*`
//! stay opaque display fields (commission math is computed on demand by
//! `modules::reports::service::employee_earnings`).

mod domain_print_jobs;

pub use domain_print_jobs::CreatePrintJobRequest;
pub use domain_print_jobs::PrintJob;
pub use domain_print_jobs::PrintJobAssignment;
pub use domain_print_jobs::PrintJobCustomer;
pub use domain_print_jobs::PrintJobListQuery;
pub use domain_print_jobs::PrintJobListResponse;
pub use domain_print_jobs::PrintJobStats;
pub use domain_print_jobs::UpdatePrintJobRequest;
