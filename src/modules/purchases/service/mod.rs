// Business rules and orchestration for the purchase / stock-intake history
// module. Calls into `super::repository` and, cross-module, into
// `suppliers::service`/`inventory::service` for all Mongo access, returning
// domain types (never BSON/`ObjectId`) to `routes`.

pub mod credit;
pub mod purchase;
pub mod receipt;
pub mod receipt_migration;
pub mod settlement;
