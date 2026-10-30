use super::list_packages;
use super::package_archive;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_packages::list_packages))
        .routes(routes!(package_archive::package_archive))
}
