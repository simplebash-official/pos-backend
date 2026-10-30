use super::cancel_quotation;
use super::create_quotation;
use super::get_quotation;
use super::get_quotation_document;
use super::get_quotation_document_design;
use super::list_quotations;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            list_quotations::list_quotations,
            create_quotation::create_quotation
        ))
        .routes(routes!(get_quotation::get_quotation))
        .routes(routes!(cancel_quotation::cancel_quotation))
        .routes(routes!(get_quotation_document::get_quotation_document))
        .routes(routes!(
            get_quotation_document_design::get_quotation_document_design
        ))
}
