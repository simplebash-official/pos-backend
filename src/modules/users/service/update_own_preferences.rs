use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::{CATALOG_SORT_IDS, UpdateMyPreferencesRequest, User};

use crate::modules::users::repository;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Merges `changes` into a login's own preferences. Callers have already
/// resolved `id` from the authenticated token, so no role hierarchy applies:
/// every login may edit its own preferences and nobody else's.
pub(crate) async fn update_own_preferences(
    db: &Db,
    id: ObjectId,
    changes: UpdateMyPreferencesRequest,
) -> AppResult<User> {
    if let Some(sort) = changes.billing_catalog_sort.as_deref()
        && !CATALOG_SORT_IDS.contains(&sort)
    {
        return Err(AppError::validation(format!(
            "Unknown billing catalog sort '{sort}'"
        )));
    }

    let existing = repository::find_user_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    let mut merged = existing.preferences.unwrap_or_default();
    if changes.billing_catalog_sort.is_some() {
        merged.billing_catalog_sort = changes.billing_catalog_sort;
    }
    let preferences_doc = mongodb::bson::serialize_to_document(&merged)
        .map_err(|e| AppError::internal(format!("failed to encode preferences: {e}")))?;

    let updated = repository::update_user(
        db,
        id,
        doc! { "preferences": preferences_doc, "updated_at": BsonDateTime::now() },
    )
    .await?
    .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;
    Ok(updated.into_user())
}
