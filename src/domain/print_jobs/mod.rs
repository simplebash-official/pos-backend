// Pure business types for the print-jobs feature — mirrors `domain::repairs`
// closely; the two modules differ only in their domain-specific fields
// (job type/quantity vs device model/issue description). See
// `domain::repairs`'s module comment for how `assigned_employee_id` is
// resolved server-side against `modules::employees` and why `split_*`
// stay opaque display fields (commission math is computed on demand by
// `modules::reports::service::employee_earnings`).

mod create_print_job_request;
mod print_job;
mod print_job_assignment;
mod print_job_customer;
mod print_job_list_query;
mod print_job_list_response;
mod print_job_stats;
mod update_print_job_request;

pub use create_print_job_request::CreatePrintJobRequest;
pub use print_job::PrintJob;
pub use print_job_assignment::PrintJobAssignment;
pub use print_job_customer::PrintJobCustomer;
pub use print_job_list_query::PrintJobListQuery;
pub use print_job_list_response::PrintJobListResponse;
pub use print_job_stats::PrintJobStats;
pub use update_print_job_request::UpdatePrintJobRequest;
