// Label print history. A real print or download goes through `create_job`,
// which renders exactly like `render_labels` and then records what was printed,
// how, and by whom; the computer reports back whether its printer step worked
// (`report_job`). `coverage` turns that history plus the stock on hand into a
// per-product "labelled / never labelled / labels needed" view. Previews are
// never recorded.

mod coverage;
mod coverage_status;
mod create_job;
mod get_job;
mod job_not_found;
mod last_label;
mod last_labels;
mod list_jobs;
mod report_job;
#[cfg(test)]
mod tests;
mod validate_job_request;

pub(crate) use coverage::coverage;
pub(crate) use create_job::create_job;
pub(crate) use get_job::get_job;
pub(crate) use list_jobs::list_jobs;
pub(crate) use report_job::report_job;
