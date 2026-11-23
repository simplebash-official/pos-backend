//! Repair-ticket CRUD, layered model (BSON shapes) -> repository (Mongo
//! only) -> service (business rules) -> routes (HTTP), same shape as
//! `modules::customers`. `service::mark_billed` and `service::clear_billed` are
//! narrow cross-module hooks `modules::billing`'s complete-sale and void flows call — the same
//! pattern as `customers::service::apply_financial_delta`.

pub mod migration;
pub mod model;
mod repository;
pub mod routes;
pub mod service;
