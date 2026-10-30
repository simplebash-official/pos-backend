use super::hash_password::hash_password;

use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::users::repository;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Sets or updates a numeric PIN for a user account.
pub async fn set_user_pin(db: &Db, user_id_or_key: &str, pin: &str) -> AppResult<()> {
    if pin.len() < 4 || pin.len() > 8 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::validation("PIN must be 4 to 8 numeric digits"));
    }
    let user_doc = if let Ok(oid) = ObjectId::parse_str(user_id_or_key) {
        repository::find_user_by_id(db, oid).await?
    } else {
        None
    };
    let user_doc = match user_doc {
        Some(u) => u,
        None => repository::find_user_by_username(db, user_id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND)
            })?,
    };

    let hashed = hash_password(pin)?;
    let oid = user_doc.id.expect("persisted user must have id");
    repository::update_user(
        db,
        oid,
        doc! { "pin_hash": hashed, "updated_at": BsonDateTime::now() },
    )
    .await?;
    Ok(())
}
