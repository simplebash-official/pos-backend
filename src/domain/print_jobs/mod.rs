//! Pure business types for the print-jobs feature — mirrors `domain::repairs`
//! closely: both are tickets priced from their items. They differ in what describes the job
//! (type, order quantity and note against device model and issue) and in their lines, which
//! carry a quantity here. See
//! `domain::repairs`'s module comment for how `assigned_employee_id` is
//! resolved server-side against `modules::employees` and why `split_*`
//! stay opaque display fields (commission math is computed on demand by
//! `modules::reports::service::employee_earnings`).

mod domain_print_jobs;
mod print_frame_size;
mod print_job_frame;
mod print_job_item;
mod print_service;

pub use domain_print_jobs::CreatePrintJobRequest;
pub use domain_print_jobs::PrintJob;
pub use domain_print_jobs::PrintJobAssignment;
pub use domain_print_jobs::PrintJobCustomer;
pub use domain_print_jobs::PrintJobListQuery;
pub use domain_print_jobs::PrintJobListResponse;
pub use domain_print_jobs::PrintJobStats;
pub use domain_print_jobs::UpdatePrintJobRequest;
pub use print_frame_size::CreatePrintFrameSizeRequest;
pub use print_frame_size::PrintFrameSize;
pub use print_frame_size::PrintFrameSizeListQuery;
pub use print_frame_size::UpdatePrintFrameSizeRequest;
pub use print_frame_size::frame_width_inches;
pub use print_frame_size::normalize_frame_size;
pub use print_frame_size::print_frame_size_key;
pub use print_job_frame::PrintJobFrame;
pub use print_job_frame::PrintJobFrameInput;
pub use print_job_item::PrintItemSource;
pub use print_job_item::PrintItemSourceInput;
pub use print_job_item::PrintJobItem;
pub use print_job_item::PrintJobItemInput;
pub use print_job_item::PrintJobKind;
pub use print_service::PrintService;
pub use print_service::PrintServiceListQuery;
pub use print_service::SavePrintServiceRequest;
pub use print_service::print_service_key;
