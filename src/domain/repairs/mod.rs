// Pure business types for the repairs feature — no I/O, no Mongo/Axum types
// beyond serde/utoipa derives. Mongo document shapes live in
// `modules::repairs::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

mod compute_job_is_overdue;
mod create_repair_request;
mod repair;
mod repair_assignment;
mod repair_customer;
mod repair_list_query;
mod repair_list_response;
mod repair_stats;
mod update_repair_request;

pub use compute_job_is_overdue::compute_job_is_overdue;
pub use create_repair_request::CreateRepairRequest;
pub use repair::Repair;
pub use repair_assignment::RepairAssignment;
pub use repair_customer::RepairCustomer;
pub use repair_list_query::RepairListQuery;
pub use repair_list_response::RepairListResponse;
pub use repair_stats::RepairStats;
pub use update_repair_request::UpdateRepairRequest;
