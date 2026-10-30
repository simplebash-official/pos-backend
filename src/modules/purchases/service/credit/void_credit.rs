use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::supplier_credits::SupplierCredit;
use crate::modules::purchases::model::SupplierCreditDocument;
use crate::modules::purchases::repository;
use crate::modules::purchases::service::receipt::RecordedBy;

use mongodb::bson::DateTime as BsonDateTime;

/// Cancels a credit by recording its opposite. Voiding twice is refused.
pub async fn void_credit(db: &Db, by: RecordedBy, key: &str) -> AppResult<SupplierCredit> {
    crate::core::logging::domain::tracked("purchases.credit_voided", async move {
        let original = repository::credit::find_credit_by_key(db, key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Credit not found", codes::CREDIT_NOT_FOUND)
            })?;
        if original.reverses_key.is_some() {
            return Err(AppError::validation_with_code(
                "A voiding entry cannot be voided.",
                codes::CREDIT_NOT_VOIDABLE,
            ));
        }
        let void_key = format!("{key}_rev");
        if repository::credit::find_credit_by_key(db, &void_key)
            .await?
            .is_some()
        {
            return Err(AppError::conflict(
                codes::CREDIT_ALREADY_VOIDED,
                "This credit was already voided.",
            ));
        }
        let now = BsonDateTime::now();
        let inserted = repository::credit::insert_credit(
            db,
            SupplierCreditDocument {
                id: None,
                key: void_key,
                supplier_key: original.supplier_key,
                receipt_key: original.receipt_key,
                amount_cents: original.amount_cents,
                reason: original.reason,
                note: Some(format!("Voids credit {key}")),
                reverses_key: Some(original.key),
                recorded_by_user_id: by.user_id,
                recorded_by_name_snapshot: by.name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        Ok(inserted.into_credit(None))
    })
    .await
}
