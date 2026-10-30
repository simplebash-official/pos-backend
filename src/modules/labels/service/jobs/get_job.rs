use super::job_not_found::job_not_found;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::labels::LabelJob;

use crate::modules::labels::model::LabelPrintJobDocument;
use crate::modules::labels::repository;

/// One recorded print.
pub(crate) async fn get_job(db: &Db, key: &str) -> AppResult<LabelJob> {
    repository::find_job_by_key(db, key)
        .await?
        .map(LabelPrintJobDocument::into_job)
        .ok_or_else(job_not_found)
}
