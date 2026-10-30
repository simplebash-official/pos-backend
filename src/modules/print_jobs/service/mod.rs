// Business rules for print-job CRUD/listing — mirrors
// `modules::repairs::service` exactly in shape; differs only in the
// domain-specific validation (job type/quantity vs device model/issue
// description).

mod create_print_job;
mod delete_print_job;
mod get_print_job;
mod get_print_job_stats;
mod hydrate_sync_documents;
mod list_print_jobs;
mod mark_delivered;
mod resolve_assignment_employee_name;
mod resolve_customer_name_phone;
mod update_print_job;
mod validate_job_type;
mod validate_promised_ready_at;
mod validate_required_fields;
mod validate_status;

pub use create_print_job::create_print_job;
pub use delete_print_job::delete_print_job;
pub use get_print_job::get_print_job;
pub use get_print_job_stats::get_print_job_stats;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_print_jobs::list_print_jobs;
pub(crate) use mark_delivered::mark_delivered;
pub use update_print_job::update_print_job;
