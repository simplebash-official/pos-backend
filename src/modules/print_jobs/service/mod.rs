//! Business rules for print-job CRUD/listing — mirrors
//! `modules::repairs::service` closely; differs in what describes the job (type, order quantity
//! and note) and in its lines, which carry a quantity.

mod print_frame_sizes_service;
mod print_job_alerts;
mod print_job_frames;
mod print_job_items;
mod print_job_reports;
mod print_jobs_service;
mod print_services_service;

pub(crate) use print_frame_sizes_service::create_frame_size;
pub(crate) use print_frame_sizes_service::hydrate_sync_frame_sizes;
pub(crate) use print_frame_sizes_service::list_frame_sizes;
pub(crate) use print_frame_sizes_service::update_frame_size;
pub(crate) use print_job_items::check_parts_in_stock;
pub(crate) use print_job_items::use_parts;
pub(crate) use print_job_reports::facts_by_job;
pub(crate) use print_job_reports::rework_costs;
pub(crate) use print_jobs_service::NewPaidJob;
pub(crate) use print_jobs_service::NewPrintRework;
pub(crate) use print_jobs_service::clear_billed;
pub(crate) use print_jobs_service::create_paid_job;
pub use print_jobs_service::create_print_job;
pub(crate) use print_jobs_service::create_rework;
pub use print_jobs_service::delete_print_job;
pub use print_jobs_service::get_print_job;
pub use print_jobs_service::get_print_job_stats;
pub(crate) use print_jobs_service::hydrate_sync_documents;
pub use print_jobs_service::list_print_jobs;
pub(crate) use print_jobs_service::mark_billed;
pub(crate) use print_jobs_service::update_print_job;
pub(crate) use print_services_service::create_service;
pub(crate) use print_services_service::hydrate_sync_services;
pub(crate) use print_services_service::list_services;
pub(crate) use print_services_service::update_service;
