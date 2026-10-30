//! Mongo and SQLite access for the `print_jobs` collection. Same never-interpret-a-
//! miss-as-an-error convention as every other repository in this codebase —
//! `service` decides what a missing row means.

mod print_jobs_repository;

pub(crate) use print_jobs_repository::aggregate_stats;
pub(crate) use print_jobs_repository::delete;

pub(crate) use print_jobs_repository::find_by_id_or_key;

pub(crate) use print_jobs_repository::insert;
pub(crate) use print_jobs_repository::list;
pub(crate) use print_jobs_repository::set_status_by_key;

pub(crate) use print_jobs_repository::update;
