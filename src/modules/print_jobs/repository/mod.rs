// Mongo and SQLite access for the `print_jobs` collection. Same never-interpret-a-
// miss-as-an-error convention as every other repository in this codebase —
// `service` decides what a missing row means.

mod aggregate_stats;
mod delete;
mod find_by_id;
mod find_by_id_or_key;
mod find_by_key;
mod insert;
mod list;
mod print_jobs;
mod row_to_print_job_doc;
mod set_status_by_key;
mod stats_aggregate_result;
mod update;

pub(crate) use aggregate_stats::aggregate_stats;
pub(crate) use delete::delete;

pub(crate) use find_by_id_or_key::find_by_id_or_key;

pub(crate) use insert::insert;
pub(crate) use list::list;
pub(crate) use set_status_by_key::set_status_by_key;

pub(crate) use update::update;
