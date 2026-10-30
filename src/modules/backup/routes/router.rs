use super::export_backup;
use super::restore_backup;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(export_backup::export_backup))
        .routes(routes!(restore_backup::restore_backup))
        .layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024))
}
