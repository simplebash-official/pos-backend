use crate::core::error::{AppError, AppResult};

use mongodb::bson::oid::ObjectId;

/// Parses a string into a MongoDB `ObjectId`, returning a `NOT_FOUND` error if invalid.
pub fn parse_object_id(id: &str, entity_name: &str) -> AppResult<ObjectId> {
    ObjectId::parse_str(id).map_err(|_| {
        let code = format!("{}_NOT_FOUND", entity_name.to_uppercase().replace(' ', "_"));
        AppError::not_found_with_code(format!("{entity_name} not found"), code)
    })
}
