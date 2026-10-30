use super::directory::directory;
use crate::clients::db::Db;
use crate::core::error::AppResult;

use mongodb::bson::doc;

/// Sets (`Some((since, reason))`) or clears (`None`) a tenant's suspension marker. `false` when no
/// such tenant exists.
pub(crate) async fn set_tenant_suspended(
    db: &Db,
    key: &str,
    suspension: Option<(mongodb::bson::DateTime, &str)>,
) -> AppResult<bool> {
    let now = mongodb::bson::DateTime::now();
    let update = match suspension {
        Some((since, reason)) => doc! {
            "$set": { "suspended_at": since, "suspended_reason": reason, "updated_at": now }
        },
        None => doc! {
            "$set": { "updated_at": now },
            "$unset": { "suspended_at": "", "suspended_reason": "" }
        },
    };
    let result = directory(db)?
        .update_one(doc! { "key": key }, update)
        .await?;
    Ok(result.matched_count > 0)
}
