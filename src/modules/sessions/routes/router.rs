use super::cancel_close;
use super::close_session;
use super::convert_variance;
use super::create_cash_movement;
use super::get_cash_movement;
use super::get_current_session;
use super::get_drawer_state;
use super::get_session;
use super::get_z_report;
use super::get_z_report_document;
use super::get_z_report_document_design;
use super::list_cash_movements;
use super::list_sessions;
use super::open_session;
use super::reopen_session;
use super::reverse_cash_movement;
use super::submit_blind_count;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(open_session::open_session))
        .routes(routes!(get_current_session::get_current_session))
        .routes(routes!(get_drawer_state::get_drawer_state))
        .routes(routes!(list_sessions::list_sessions))
        .routes(routes!(create_cash_movement::create_cash_movement))
        .routes(routes!(reverse_cash_movement::reverse_cash_movement))
        .routes(routes!(list_cash_movements::list_cash_movements))
        .routes(routes!(get_cash_movement::get_cash_movement))
        .routes(routes!(submit_blind_count::submit_blind_count))
        .routes(routes!(cancel_close::cancel_close))
        .routes(routes!(close_session::close_session))
        .routes(routes!(reopen_session::reopen_session))
        .routes(routes!(convert_variance::convert_variance))
        .routes(routes!(get_z_report::get_z_report))
        .routes(routes!(get_z_report_document::get_z_report_document))
        .routes(routes!(
            get_z_report_document_design::get_z_report_document_design
        ))
        .routes(routes!(get_session::get_session))
}
