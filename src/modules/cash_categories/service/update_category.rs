use super::clean_name::clean_name;
use super::clean_name_si::clean_name_si;
use super::other_category_key::other_category_key;
use super::require_category::require_category;
use super::usage_by_key::usage_by_key;
use super::usage_of::usage_of;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::cash_categories::{
    CashCategory, ReviewStatus, UpdateCashCategoryRequest, normalize_name,
};
use crate::modules::cash_categories::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Renames, hides, pins, reorders, sets receipt/approval rules on, or approves a category.
pub async fn update_category(
    db: &Db,
    key: &str,
    body: UpdateCashCategoryRequest,
) -> AppResult<CashCategory> {
    tracked("cash_categories.updated", async move {
        let mut doc = require_category(db, key).await?;
        if doc.merged_into_key.is_some() {
            return Err(AppError::conflict(
                codes::CASH_CATEGORY_HIDDEN,
                "This category was merged into another one and can no longer be changed.",
            ));
        }
        let is_other = doc.key == other_category_key(doc.movement_type);

        if let Some(raw) = &body.name {
            let name = clean_name(raw)?;
            let normalized = normalize_name(&name);
            if is_other && normalized != doc.normalized_name {
                return Err(AppError::validation(
                    "The Other category keeps its name, because one-off labels are filed under it.",
                ));
            }
            if repository::list_all(db).await?.iter().any(|c| {
                c.key != doc.key && c.movement_type == doc.movement_type && c.normalized_name == normalized
            }) {
                return Err(AppError::conflict(
                    codes::CASH_CATEGORY_NAME_TAKEN,
                    "A category with this name already exists.",
                ));
            }
            doc.name = name;
            doc.normalized_name = normalized;
        }
        if let Some(raw) = &body.name_si {
            doc.name_si = clean_name_si(raw)?;
        }
        if let Some(group) = body.report_group {
            doc.report_group = group;
        }
        if let Some(pinned) = body.pinned {
            doc.pinned = pinned;
        }
        if let Some(sort_order) = body.sort_order {
            doc.sort_order = sort_order;
        }
        if let Some(active) = body.active {
            if !active && is_other {
                return Err(AppError::validation(
                    "The Other category cannot be hidden, because one-off labels are filed under it.",
                ));
            }
            doc.active = active;
        }
        if let Some(required) = body.requires_receipt {
            doc.requires_receipt = required;
        }
        if let Some(limit) = body.requires_approval_over_cents {
            if limit < 0 {
                return Err(AppError::validation("The approval limit cannot be negative."));
            }
            doc.requires_approval_over_cents = (limit > 0).then_some(limit);
        }
        if body.approve == Some(true) {
            doc.review_status = ReviewStatus::Approved;
        }

        doc.version += 1;
        doc.updated_at = BsonDateTime::now();
        repository::save(db, &doc).await?;
        let all = repository::list_all(db).await?;
        let usage = usage_by_key(db).await?;
        let n = usage_of(&doc, &all, &usage);
        Ok(doc.into_cash_category(n))
    })
    .await
}
