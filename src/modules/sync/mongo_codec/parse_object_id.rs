use crate::core::error::{AppError, AppResult};

use mongodb::bson::oid::ObjectId;

pub(crate) fn parse_object_id(text: &str) -> AppResult<ObjectId> {
    ObjectId::parse_str(text).map_err(|_| AppError::validation("payload id is not a valid id"))
}
