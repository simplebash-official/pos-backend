use super::create_category;
use super::list_categories;
use super::merge_category;
use super::similar_categories;
use super::update_category;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_categories::list_categories))
        .routes(routes!(similar_categories::similar_categories))
        // Writes
        .routes(routes!(create_category::create_category))
        .routes(routes!(update_category::update_category))
        .routes(routes!(merge_category::merge_category))
}
