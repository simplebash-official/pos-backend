// Repair-ticket CRUD, layered model (BSON shapes) -> repository (Mongo
// only) -> service (business rules) -> routes (HTTP), same shape as
// `modules::customers`. `service::mark_delivered` is a narrow cross-module
// hook `modules::billing`'s complete-sale flow calls (Phase 4) — the same
// pattern as `customers::service::apply_financial_delta`.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
