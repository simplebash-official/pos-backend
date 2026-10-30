use super::get_document_template_preview;
use super::get_document_templates;
use super::get_shop_profile;
use super::update_document_templates;
use super::update_shop_profile;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            get_shop_profile::get_shop_profile,
            update_shop_profile::update_shop_profile
        ))
        .routes(routes!(
            get_document_templates::get_document_templates,
            update_document_templates::update_document_templates
        ))
        .routes(routes!(
            get_document_template_preview::get_document_template_preview
        ))
}
