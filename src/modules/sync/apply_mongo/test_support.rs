//! Test support for MongoDB sync apply.

use mongodb::bson::Document;
use serde_json::Value;

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::modules::sync::mongo_codec;
use crate::modules::sync::service::hydrate;

/// The wire DTO of one stored document.
pub async fn hydrate_for_test(db: &Db, resource: &str, document: Document) -> AppResult<Value> {
    hydrate(db, resource, vec![document])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("nothing hydrated"))
}

/// The stored fields (ordinary + derived + `_id`) a payload would produce.
pub fn encode_for_test(resource: &str, payload: &Value) -> AppResult<Document> {
    let spec = crate::modules::sync::resources::spec(resource)
        .ok_or_else(|| AppError::internal("unknown resource"))?;
    let encoded = mongo_codec::encode(spec, payload)?;
    let mut all = Document::new();
    all.insert("_id", encoded.id);
    all.extend(encoded.fields);
    all.extend(encoded.derived);
    Ok(all)
}
