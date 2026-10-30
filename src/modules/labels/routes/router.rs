use super::coverage;
use super::create_job;
use super::create_template;
use super::delete_template;
use super::get_job;
use super::list_jobs;
use super::list_templates;
use super::preview_label;
use super::render_labels;
use super::report_job;
use super::update_template;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Designs
        .routes(routes!(
            list_templates::list_templates,
            create_template::create_template
        ))
        .routes(routes!(
            update_template::update_template,
            delete_template::delete_template
        ))
        // Printing
        .routes(routes!(render_labels::render_labels))
        .routes(routes!(preview_label::preview_label))
        // Print jobs
        .routes(routes!(list_jobs::list_jobs, create_job::create_job))
        .routes(routes!(get_job::get_job, report_job::report_job))
        .routes(routes!(coverage::coverage))
}
