// HTTP layer only: extractors, permission checks and OpenAPI docs. Every
// handler makes one `service::*` call; the rules live in `service/`.

mod coverage;
mod create_job;
mod create_template;
mod delete_template;
mod get_job;
mod label_job_key_header;
mod list_jobs;
mod list_templates;
mod pdf_response;
mod preview_label;
mod render_labels;
mod report_job;
mod router;
mod update_template;

pub use label_job_key_header::LABEL_JOB_KEY_HEADER;
pub use router::router;
