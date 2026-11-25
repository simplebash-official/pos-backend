//! Accounts stored on the cloud without a version (MongoDB).

use super::ledger_backfill::Report;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::modules::inventory::service::stock_batch_migration::Mode;
use crate::modules::sync::cloud_capture;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

/// Gives version 1 to every shop account that has none. The write itself carries the account to
/// the shop's devices: the running server's change stream publishes it. A repeat finds nothing.
pub async fn migrate_user_versions(db: &Db, mode: Mode) -> AppResult<Report> {
    let users = db
        .as_mongo()
        .ok_or_else(|| AppError::internal("the user version backfill requires MongoDB"))?
        .unscoped()
        .collection::<Document>("users");
    let mut report = Report::default();
    let mut unnumbered = users.find(doc! { "version": { "$exists": false } }).await?;
    while let Some(account) = unnumbered.try_next().await? {
        let Some(shop) = cloud_capture::shop_of(&account).map(str::to_owned) else {
            continue;
        };
        if mode == Mode::Apply {
            let id = account
                .get_object_id("_id")
                .map_err(|err| AppError::internal(format!("stored user has no _id: {err}")))?;
            users
                .update_one(
                    doc! { "_id": id, "version": { "$exists": false } },
                    doc! { "$set": { "version": 1_i64 } },
                )
                .await?;
        }
        *report.shops.entry(shop).or_insert(0) += 1;
    }
    Ok(report)
}
