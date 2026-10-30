use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::supplier_credits::{CreateSupplierCreditRequest, SupplierCredit};
use crate::modules::purchases::model::SupplierCreditDocument;
use crate::modules::purchases::repository;
use crate::modules::purchases::service::receipt::{RecordedBy, get_receipt};
use crate::modules::suppliers::service as supplier_service;
use mongodb::bson::DateTime as BsonDateTime;

const NOTE_MAX: usize = 200;

/// Records a credit from a supplier. Against a receipt, it may not exceed what that receipt still
/// owes; with no receipt it is credit on the supplier's account.
pub async fn create_credit(
    db: &Db,
    by: RecordedBy,
    body: CreateSupplierCreditRequest,
) -> AppResult<SupplierCredit> {
    crate::core::logging::domain::tracked("purchases.credit_recorded", async move {
        if body.amount_cents <= 0 {
            return Err(AppError::validation("The credit must be more than zero"));
        }
        let note = body
            .note
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());
        if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_MAX) {
            return Err(AppError::validation(format!(
                "The note must be at most {NOTE_MAX} characters"
            )));
        }
        let supplier = supplier_service::get_supplier_by_key(db, &body.supplier_key).await?;
        if let Some(receipt_key) = &body.receipt_key {
            let receipt = get_receipt(db, receipt_key).await?;
            if receipt.supplier_key != supplier.key {
                return Err(AppError::validation(
                    "That receipt belongs to another supplier",
                ));
            }
            if body.amount_cents > receipt.due_cents {
                return Err(AppError::conflict(
                    codes::RECEIPT_OVERPAID,
                    "That is more than the receipt still owes.",
                ));
            }
        }

        let now = BsonDateTime::now();
        let inserted = repository::credit::insert_credit(
            db,
            SupplierCreditDocument {
                id: None,
                key: generate_id(prefixes::SUPPLIER_CREDIT),
                supplier_key: supplier.key,
                receipt_key: body.receipt_key,
                amount_cents: body.amount_cents,
                reason: body.reason,
                note,
                reverses_key: None,
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
