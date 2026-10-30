// HTTP route handlers and OpenAPI declarations for settings and shop profiles.

mod get_document_template_preview;
mod get_document_templates;
mod get_shop_profile;
mod router;
mod update_document_templates;
mod update_shop_profile;

pub use get_document_template_preview::get_document_template_preview;
pub use get_document_templates::get_document_templates;
pub use get_shop_profile::get_shop_profile;
pub use router::router;
pub use update_document_templates::update_document_templates;
pub use update_shop_profile::update_shop_profile;
