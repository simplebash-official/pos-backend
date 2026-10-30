// Business rules for repair-ticket CRUD/listing: required-field validation,
// status validation, ticket-number reservation on create, and the narrow
// `mark_delivered` cross-module hook. Delegates all Mongo access to
// `super::repository`.

mod create_repair;
mod delete_repair;
mod get_repair;
mod get_repair_stats;
mod hydrate_sync_documents;
mod list_repairs;
mod mark_delivered;
mod resolve_assignment_employee_name;
mod resolve_customer_name_phone;
mod update_repair;
mod validate_price_required_for_status;
mod validate_promised_ready_at;
mod validate_required_fields;
mod validate_status;

pub use create_repair::create_repair;
pub use delete_repair::delete_repair;
pub use get_repair::get_repair;
pub use get_repair_stats::get_repair_stats;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_repairs::list_repairs;
pub(crate) use mark_delivered::mark_delivered;
pub use update_repair::update_repair;
