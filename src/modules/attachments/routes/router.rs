use super::get_attachment_content;
use super::list_attachments;
use super::upload_attachment;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_attachments::list_attachments))
        .routes(routes!(get_attachment_content::get_attachment_content))
        // Writes
        .routes(routes!(upload_attachment::upload_attachment))
}
