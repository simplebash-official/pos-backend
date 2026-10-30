//! Mongo and SQLite access for the `label_templates` and `label_print_jobs`
//! collections/tables. Like every repository it never turns a miss into an
//! error: `service` decides what a missing template or job means.

mod labels_repository;

pub(crate) use labels_repository::JobFilter;
pub(crate) use labels_repository::LabelledEntry;
pub(crate) use labels_repository::delete_template;
pub(crate) use labels_repository::find_job_by_key;
pub(crate) use labels_repository::find_template_by_key;
pub(crate) use labels_repository::insert_job;
pub(crate) use labels_repository::insert_template;
pub(crate) use labels_repository::list_jobs;
pub(crate) use labels_repository::list_labelled_entries;
pub(crate) use labels_repository::list_templates;
pub(crate) use labels_repository::report_job_outcome;
pub(crate) use labels_repository::update_template;
