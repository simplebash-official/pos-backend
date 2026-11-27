//! Business rules for repair-ticket CRUD/listing: required-field validation,
//! status validation, ticket-number reservation on create, and the narrow
//! `mark_billed` / `clear_billed` cross-module hooks. Delegates all Mongo access to
//! `super::repository`.

mod repair_alerts;
mod repair_items;
mod repair_reports;
mod repair_services_service;
mod repairs_service;

pub(crate) use repair_items::check_parts_in_stock;
pub(crate) use repair_items::use_parts;
pub(crate) use repair_reports::rework_costs;
pub(crate) use repair_reports::technicians_by_job;
pub(crate) use repair_services_service::create_service;
pub(crate) use repair_services_service::hydrate_sync_services;
pub(crate) use repair_services_service::list_services;
pub(crate) use repair_services_service::update_service;
pub(crate) use repairs_service::NewRework;
pub(crate) use repairs_service::clear_billed;
pub use repairs_service::create_repair;
pub(crate) use repairs_service::create_rework;
pub use repairs_service::delete_repair;
pub use repairs_service::get_repair;
pub use repairs_service::get_repair_stats;
pub(crate) use repairs_service::hydrate_sync_documents;
pub use repairs_service::list_repairs;
pub(crate) use repairs_service::mark_billed;
pub(crate) use repairs_service::update_repair;
