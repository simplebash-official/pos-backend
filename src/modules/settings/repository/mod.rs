//! Dual-engine (Mongo / SQLite) access for settings and shop profiles.

mod settings_repository;

pub(crate) use settings_repository::find_document_template_settings;
pub(crate) use settings_repository::find_shop_profile;
pub(crate) use settings_repository::upsert_document_template_settings;
pub(crate) use settings_repository::upsert_shop_profile;
