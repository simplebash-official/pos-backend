//! The cloud side of the catch-up (MongoDB).

use super::ledger_resources::ledger_specs;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::modules::inventory::service::stock_batch_migration::Mode;
use crate::modules::sync::cloud_capture;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use std::collections::BTreeMap;

/// What a run found (and, unless it was a dry run, published).
#[derive(Debug, Default)]
pub struct Report {
    /// Ledger documents per shop id.
    pub shops: BTreeMap<String, u64>,
}

/// Publishes every ledger document of every shop to that shop's change feed, so devices receive
/// the rows written before the cloud could send them. Repeating a run adds nothing: the feed keeps
/// one change per document revision from this source.
pub async fn migrate_mongo(db: &Db, mode: Mode) -> AppResult<Report> {
    let raw = db
        .as_mongo()
        .ok_or_else(|| AppError::internal("the ledger backfill requires MongoDB"))?
        .unscoped()
        .clone();
    let mut report = Report::default();
    for spec in ledger_specs() {
        let mut documents = raw.collection::<Document>(spec.table).find(doc! {}).await?;
        while let Some(document) = documents.try_next().await? {
            let Some(shop) = cloud_capture::shop_of(&document).map(str::to_owned) else {
                continue;
            };
            if mode == Mode::Apply {
                cloud_capture::republish(db, spec.table, &shop, document, "backfill").await?;
            }
            *report.shops.entry(shop).or_insert(0) += 1;
        }
    }
    Ok(report)
}
