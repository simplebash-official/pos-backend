//! Mongo and SQLite access for the `repairs` collection. Same never-interpret-a-
//! miss-as-an-error convention as every other repository in this codebase —
//! `service` decides what a missing row means.

pub(crate) mod repair_services_repository;
mod repairs_repository;

pub(crate) use repairs_repository::aggregate_stats;
pub(crate) use repairs_repository::delete;

pub(crate) use repairs_repository::find_by_id_or_key;

pub(crate) use repairs_repository::insert;
pub(crate) use repairs_repository::list;
pub(crate) use repairs_repository::set_status_by_key;

pub(crate) use repairs_repository::update;
