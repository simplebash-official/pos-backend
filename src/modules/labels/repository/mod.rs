// Mongo and SQLite access for the `label_templates` and `label_print_jobs`
// collections/tables. Like every repository it never turns a miss into an
// error: `service` decides what a missing template or job means.

mod collection;
mod delete_template;
mod enum_text;
mod find_job_by_key;
mod find_template_by_key;
mod insert_job;
mod insert_template;
mod job_columns;
mod job_err;
mod job_filter;
mod jobs;
mod labelled_entry;
mod list_jobs;
mod list_labelled_entries;
mod list_templates;
mod report_job_outcome;
mod row_to_doc;
mod row_to_job;
mod store_err;
mod update_template;

pub(crate) use delete_template::delete_template;
pub(crate) use find_job_by_key::find_job_by_key;
pub(crate) use find_template_by_key::find_template_by_key;
pub(crate) use insert_job::insert_job;
pub(crate) use insert_template::insert_template;
pub(crate) use job_filter::JobFilter;
pub(crate) use labelled_entry::LabelledEntry;
pub(crate) use list_jobs::list_jobs;
pub(crate) use list_labelled_entries::list_labelled_entries;
pub(crate) use list_templates::list_templates;
pub(crate) use report_job_outcome::report_job_outcome;
pub(crate) use update_template::update_template;
