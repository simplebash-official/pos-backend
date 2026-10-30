use super::delete_link;
use super::list_links;
use super::replace_links_for_supplier;
use super::upsert_link;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_links::list_links, upsert_link::upsert_link))
        .routes(routes!(delete_link::delete_link))
        .routes(routes!(
            replace_links_for_supplier::replace_links_for_supplier
        ))
}
