//! HTTP route handlers and OpenAPI declarations for settings and shop profiles.

mod settings_routes;

pub use settings_routes::get_document_template_preview;
pub use settings_routes::get_document_templates;
pub use settings_routes::get_shop_profile;
pub use settings_routes::router;
pub use settings_routes::update_document_templates;
pub use settings_routes::update_shop_profile;
