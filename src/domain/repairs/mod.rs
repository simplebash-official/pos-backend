//! Pure business types for the repairs feature — no I/O, no Mongo/Axum types
//! beyond serde/utoipa derives. Mongo document shapes live in
//! `modules::repairs::model` and convert into these before a handler wraps
//! them in `core::response::ApiResponse<T>`.

mod domain_repairs;

pub use domain_repairs::CreateRepairRequest;
pub use domain_repairs::Repair;
pub use domain_repairs::RepairAssignment;
pub use domain_repairs::RepairCustomer;
pub use domain_repairs::RepairListQuery;
pub use domain_repairs::RepairListResponse;
pub use domain_repairs::RepairStats;
pub use domain_repairs::UpdateRepairRequest;
pub use domain_repairs::compute_job_is_overdue;
