//! Label print history. A real print or download goes through `create_job`,
//! which renders exactly like `render_labels` and then records what was printed,
//! how, and by whom; the computer reports back whether its printer step worked
//! (`report_job`). `coverage` turns that history plus the stock on hand into a
//! per-product "labelled / never labelled / labels needed" view. Previews are
//! never recorded.

mod service_jobs;
#[cfg(test)]
mod tests;

pub(crate) use service_jobs::coverage;
pub(crate) use service_jobs::create_job;
pub(crate) use service_jobs::get_job;
pub(crate) use service_jobs::list_jobs;
pub(crate) use service_jobs::report_job;
