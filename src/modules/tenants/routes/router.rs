use super::batch_usage;
use super::provision_shop;
use super::reset_shop_document_selections;
use super::set_status;
use super::shop_activities;
use super::shop_document_selections;
use super::shop_logins;
use super::shop_usage;
use super::shop_users;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(provision_shop::provision_shop))
        .routes(routes!(set_status::set_status))
        .routes(routes!(shop_usage::shop_usage))
        .routes(routes!(batch_usage::batch_usage))
        .routes(routes!(shop_document_selections::shop_document_selections))
        .routes(routes!(
            reset_shop_document_selections::reset_shop_document_selections
        ))
        .routes(routes!(shop_users::shop_users))
        .routes(routes!(shop_logins::shop_logins))
        .routes(routes!(shop_activities::shop_activities))
}
