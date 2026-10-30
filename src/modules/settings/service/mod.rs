//! Business logic for settings and shop profile persistence.

mod settings_service;

pub use settings_service::SHOP_PROFILE_KEY;
pub(crate) use settings_service::get_document_template_settings;
pub(crate) use settings_service::get_document_templates;
pub(crate) use settings_service::get_shop_profile;
pub use settings_service::list_document_selections;
pub(crate) use settings_service::render_document_template_preview;
pub use settings_service::reset_document_families;
pub(crate) use settings_service::update_document_templates;
pub(crate) use settings_service::update_shop_profile;
