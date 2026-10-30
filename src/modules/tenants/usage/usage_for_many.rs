use super::max_batch::MAX_BATCH;
use super::tenant_usage::TenantUsage;
use super::usage_for::usage_for;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use futures_util::{StreamExt, stream};

const CONCURRENCY: usize = 8;

/// Usage of several shops at once (a shop that is not in the directory is simply left out), a few
/// reads at a time so one call cannot flood MongoDB.
pub async fn usage_for_many(db: &Db, tenant_ids: &[String]) -> AppResult<Vec<TenantUsage>> {
    if tenant_ids.len() > MAX_BATCH {
        return Err(AppError::validation(format!(
            "at most {MAX_BATCH} shops per request"
        )));
    }
    // Futures are built first and polled a few at a time; mapping inside the stream trips a
    // higher-ranked lifetime error once the handler is boxed by `routes!`.
    let pending: Vec<_> = tenant_ids.iter().map(|id| usage_for(db, id)).collect();
    let results: Vec<AppResult<TenantUsage>> =
        stream::iter(pending).buffered(CONCURRENCY).collect().await;
    let mut out = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(usage) => out.push(usage),
            Err(AppError::NotFound { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}
