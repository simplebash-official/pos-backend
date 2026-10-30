use super::clean_name::clean_name;
use super::clean_phone::clean_phone;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::cash_categories::{Payee, UpdatePayeeRequest, normalize_name};
use crate::modules::payees::model::PayeeDocument;
use crate::modules::payees::repository;
use crate::modules::sessions;
use mongodb::bson::DateTime as BsonDateTime;

async fn require_payee(db: &Db, key: &str) -> AppResult<PayeeDocument> {
    repository::find_by_key(db, key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Payee not found.", codes::PAYEE_NOT_FOUND))
}

/// Renames, edits the phone of, or hides/shows a landlord, company or saved name. Suppliers and
/// staff are changed on their own pages, so only stored payees (`pye_…`) are found here.
pub async fn update_payee(db: &Db, key: &str, body: UpdatePayeeRequest) -> AppResult<Payee> {
    tracked("payees.updated", async move {
        if key.starts_with(&format!("{}_", prefixes::SUPPLIER))
            || key.starts_with(&format!("{}_", prefixes::EMPLOYEE))
        {
            return Err(AppError::validation(
                "Suppliers and staff are changed on their own pages.",
            ));
        }
        let mut doc = require_payee(db, key).await?;

        if let Some(raw) = &body.name {
            let name = clean_name(raw)?;
            let normalized = normalize_name(&name);
            if repository::list_all(db).await?.iter().any(|p| {
                p.key != doc.key
                    && p.payee_type == doc.payee_type
                    && p.normalized_name == normalized
            }) {
                return Err(AppError::conflict(
                    codes::PAYEE_NAME_TAKEN,
                    "Someone with this name is already in the list.",
                ));
            }
            doc.name = name;
            doc.normalized_name = normalized;
        }
        if let Some(raw) = &body.phone {
            doc.phone = clean_phone(Some(raw));
        }
        if let Some(active) = body.active {
            doc.active = active;
        }

        doc.version += 1;
        doc.updated_at = BsonDateTime::now();
        repository::save(db, &doc).await?;
        let usage = sessions::service::movement_counts_by_payee(db)
            .await?
            .into_iter()
            .find(|(k, _)| *k == doc.key)
            .map_or(0, |(_, n)| n);
        Ok(Payee {
            key: doc.key,
            payee_type: doc.payee_type,
            name: doc.name,
            phone: doc.phone,
            usage_count: usage,
            active: doc.active,
        })
    })
    .await
}
