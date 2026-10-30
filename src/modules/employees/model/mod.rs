// Mongo document shape for the employees feature. Kept separate from
// `domain::employees` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads. Shaped like
// `modules::suppliers::model::SupplierDocument` (version/soft-delete/
// updated_by_device, sync-ready) rather than `modules::users::model::
// UserDocument` (no version, hard delete) — an Employee is a synced
// resource (see `modules::sync::service::SYNCABLE`), a User login never is.

mod employee_document;

pub use employee_document::EmployeeDocument;
