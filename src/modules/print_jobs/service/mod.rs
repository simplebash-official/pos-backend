//! Business rules for print-job CRUD/listing — mirrors
//! `modules::repairs::service` exactly in shape; differs only in the
//! domain-specific validation (job type/quantity vs device model/issue
//! description).

mod print_job_alerts;
mod print_jobs_service;

pub(crate) use print_jobs_service::clear_billed;
pub use print_jobs_service::create_print_job;
pub use print_jobs_service::delete_print_job;
pub use print_jobs_service::get_print_job;
pub use print_jobs_service::get_print_job_stats;
pub(crate) use print_jobs_service::hydrate_sync_documents;
pub use print_jobs_service::list_print_jobs;
pub(crate) use print_jobs_service::mark_billed;
pub use print_jobs_service::update_print_job;
