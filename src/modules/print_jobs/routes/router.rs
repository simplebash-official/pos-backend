use super::create_print_job;
use super::delete_print_job;
use super::get_print_job;
use super::get_print_job_stats;
use super::list_print_jobs;
use super::update_print_job;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            list_print_jobs::list_print_jobs,
            create_print_job::create_print_job
        ))
        .routes(routes!(get_print_job_stats::get_print_job_stats))
        .routes(routes!(
            get_print_job::get_print_job,
            update_print_job::update_print_job,
            delete_print_job::delete_print_job
        ))
}
