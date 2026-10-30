use crate::core::error::AppResult;

use crate::core::utils::parse_object_id as parse_mongo_id;

use mongodb::bson::oid::ObjectId;

/// Parses a `Path<String>` id param — this is the only place in the file
/// that turns a raw path segment into an `ObjectId`; every handler that
/// needs one calls this before delegating to `service::*`.
pub(super) fn parse_object_id(id: &str) -> AppResult<ObjectId> {
    parse_mongo_id(id, "Supplier")
}
