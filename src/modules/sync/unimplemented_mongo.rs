use crate::core::error::{AppError, AppResult};
use crate::domain::sync_v2::ApplyOutcome;

/// Placeholder body for the `Db::Mongo` arm of a per-module
/// `apply_sync_documents` until the cloud package fills it in.
#[allow(dead_code)]
pub(crate) fn unimplemented_mongo() -> AppResult<ApplyOutcome> {
    Err(AppError::internal(
        "sync apply is not implemented for MongoDB yet",
    ))
}
