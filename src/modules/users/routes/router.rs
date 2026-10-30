use super::create_user;
use super::delete_user;
use super::get_user;
use super::list_users;
use super::update_user;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Users
        .routes(routes!(list_users::list_users, create_user::create_user))
        .routes(routes!(
            get_user::get_user,
            update_user::update_user,
            delete_user::delete_user
        ))
}
