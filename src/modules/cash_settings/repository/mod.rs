//! Dual-engine (Mongo / SQLite) access for the single `cash_settings` row.

mod cash_settings_repository;

pub(crate) use cash_settings_repository::find;
pub(crate) use cash_settings_repository::insert;
pub(crate) use cash_settings_repository::save;
