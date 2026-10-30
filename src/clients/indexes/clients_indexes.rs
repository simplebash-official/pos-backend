use super::index_specs::index_specs;
use crate::core::config::TenantMode;
use mongodb::bson::Document;
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use std::time::Duration;

fn to_model(spec: &IndexSpec) -> IndexModel {
    let mut options = IndexOptions::builder()
        .name(spec.name.clone())
        .unique(spec.unique.then_some(true))
        .build();
    options.expire_after = spec.ttl;
    IndexModel::builder()
        .keys(spec.keys.clone())
        .options(options)
        .build()
}

/// Best-effort: a failure here is logged and the server still starts. An
/// index that could not be created makes sync slower, not wrong, and
/// refusing to boot over it would take the shop offline for a problem that
/// resolves itself on the next deploy.
pub async fn ensure_indexes(db: &Database, tenant_mode: TenantMode) {
    for spec in index_specs(tenant_mode) {
        if let Err(err) = db
            .collection::<Document>(spec.collection)
            .create_index(to_model(&spec))
            .await
        {
            tracing::warn!(
                collection = spec.collection,
                index = %spec.name,
                %err,
                "could not create index"
            );
        }
    }

    tracing::info!("Database indexes ensured");
}

/// One index to ensure: enough to build an `IndexModel` and to assert on in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexSpec {
    pub collection: &'static str,
    pub name: String,
    pub keys: Document,
    pub unique: bool,
    pub ttl: Option<Duration>,
}
