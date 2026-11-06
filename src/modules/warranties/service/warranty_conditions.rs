//! Managing shop warranty condition checklist templates.

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::warranties::{
    DEFAULT_CONDITIONS, WarrantyCondition, WarrantyConditionsResponse, condition_template_key,
};
use crate::modules::warranties::model::WarrantyConditionDocument;
use crate::modules::warranties::repository;
use chrono::Utc;
use mongodb::bson::DateTime as BsonDateTime;

/// Lists all active conditions for the shop. If no custom conditions exist yet, returns the 4
/// built-in starter fallbacks.
pub(crate) async fn list_conditions(db: &Db) -> AppResult<WarrantyConditionsResponse> {
    let docs = repository::list_active_conditions(db).await?;
    if docs.is_empty() {
        let now = Utc::now();
        let fallback = DEFAULT_CONDITIONS
            .iter()
            .map(|&name| WarrantyCondition {
                key: condition_template_key(name),
                name: name.to_string(),
                active: true,
                created_at: now,
            })
            .collect();
        return Ok(WarrantyConditionsResponse {
            conditions: fallback,
        });
    }
    let conditions = docs
        .into_iter()
        .map(WarrantyConditionDocument::into_condition)
        .collect();
    Ok(WarrantyConditionsResponse { conditions })
}

/// Creates or activates a condition template for the shop.
pub(crate) async fn create_condition(db: &Db, raw_name: &str) -> AppResult<WarrantyCondition> {
    let name = raw_name.trim();
    if name.is_empty() {
        return Err(AppError::validation("Write the condition name."));
    }
    if name.len() > 100 {
        return Err(AppError::validation(
            "Condition name is too long (at most 100 characters).",
        ));
    }
    let key = condition_template_key(name);
    let now = BsonDateTime::now();
    if let Some(mut existing) = repository::find_condition_by_key(db, &key).await? {
        if !existing.active {
            let expected = existing.version;
            existing.active = true;
            existing.name = name.to_string();
            existing.version = expected + 1;
            existing.updated_at = now;
            repository::replace_condition(db, &existing, expected).await?;
        }
        return Ok(existing.into_condition());
    }
    let doc = WarrantyConditionDocument {
        id: None,
        key,
        name: name.to_string(),
        active: true,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let saved = repository::insert_condition(db, doc).await?;
    Ok(saved.into_condition())
}

/// Ensures a condition is saved if it doesn't already exist (used during claim intake auto-upsert).
pub(crate) async fn record_condition_if_new(db: &Db, raw_name: &str) -> AppResult<()> {
    let name = raw_name.trim();
    if name.is_empty() || name.len() > 100 {
        return Ok(());
    }
    let key = condition_template_key(name);
    if let Some(existing) = repository::find_condition_by_key(db, &key).await? {
        if !existing.active {
            let mut updated = existing.clone();
            let expected = existing.version;
            updated.active = true;
            updated.version = expected + 1;
            updated.updated_at = BsonDateTime::now();
            let _ = repository::replace_condition(db, &updated, expected).await;
        }
        return Ok(());
    }
    let now = BsonDateTime::now();
    let doc = WarrantyConditionDocument {
        id: None,
        key,
        name: name.to_string(),
        active: true,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let _ = repository::insert_condition(db, doc).await;
    Ok(())
}

/// Deactivates a condition template by key so it no longer appears in future intake options.
pub(crate) async fn delete_condition(db: &Db, key: &str) -> AppResult<WarrantyCondition> {
    let existing = repository::find_condition_by_key(db, key)
        .await?
        .ok_or_else(|| AppError::not_found("Condition not found."))?;
    if !existing.active {
        return Ok(existing.into_condition());
    }
    let mut updated = existing.clone();
    let expected = existing.version;
    updated.active = false;
    updated.version = expected + 1;
    updated.updated_at = BsonDateTime::now();
    repository::replace_condition(db, &updated, expected).await?;
    Ok(updated.into_condition())
}
