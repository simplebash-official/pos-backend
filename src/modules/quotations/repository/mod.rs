//! Mongo and SQLite access for the `quotations` collection/table. Same
//! never-interpret-a-miss-as-an-error convention as every other repository —
//! `service` decides what a missing row means.

mod quotations_repository;

pub(crate) use quotations_repository::cancel;

pub(crate) use quotations_repository::find_by_id_or_key;

pub(crate) use quotations_repository::ListFilter;
pub(crate) use quotations_repository::fulfill;
pub(crate) use quotations_repository::insert;
pub(crate) use quotations_repository::list;
