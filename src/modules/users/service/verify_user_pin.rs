use super::verify_password::verify_password;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::users::{Role, User};

use crate::modules::users::repository;

use mongodb::bson::doc;

/// Verifies a numeric PIN against active users with the required role.
pub async fn verify_user_pin(
    db: &Db,
    pin: &str,
    required_role: Option<&str>,
) -> AppResult<Option<User>> {
    let filter = doc! { "is_active": true };
    let users = repository::list_users(db, filter).await?;
    for user_doc in users {
        let Some(pin_hash) = &user_doc.pin_hash else {
            continue;
        };
        if let Some(req_role) = required_role {
            match req_role.to_lowercase().as_str() {
                "owner" | "admin" => {
                    if user_doc.role != Role::Admin {
                        continue;
                    }
                }
                "manager" if user_doc.role != Role::Admin && user_doc.role != Role::Manager => {
                    continue;
                }
                _ => {}
            }
        }
        if verify_password(pin, pin_hash).unwrap_or(false) {
            return Ok(Some(user_doc.into_user()));
        }
    }
    Ok(None)
}
