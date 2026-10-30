use super::get_import_batch;
use super::list_import_batches;
use super::process_import;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            process_import::process_import,
            list_import_batches::list_import_batches
        ))
        .routes(routes!(get_import_batch::get_import_batch))
}
