use super::clean_name::clean_name;
use super::clean_phone::clean_phone;
use super::new_document::new_document;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::cash_categories::{CreatePayeeRequest, Payee, PayeeType};
use crate::modules::payees::repository;

/// Adds a landlord, utility company or other payee on purpose (a manager's job).
pub async fn create_payee(db: &Db, body: CreatePayeeRequest) -> AppResult<Payee> {
    tracked("payees.created", async move {
        if !matches!(body.payee_type, PayeeType::Vendor | PayeeType::Other) {
            return Err(AppError::validation(
                "Suppliers and staff are added on their own pages.",
            ));
        }
        let name = clean_name(&body.name)?;
        let phone = clean_phone(body.phone.as_deref());
        let document = new_document(body.payee_type, name, phone);
        if let Some(existing) = repository::list_all(db)
            .await?
            .iter()
            .find(|p| p.payee_type == document.payee_type && p.normalized_name == document.normalized_name)
        {
            return Err(if existing.active {
                AppError::conflict(codes::PAYEE_NAME_TAKEN, "Someone with this name is already in the list.")
            } else {
                AppError::conflict(
                    codes::PAYEE_HIDDEN,
                    "Someone with this name exists but is hidden. Show them again instead of adding them.",
                )
            });
        }
        // The key is derived from the name at creation and never changes, so a renamed payee
        // still owns its old key.
        if repository::find_by_key(db, &document.key).await?.is_some() {
            return Err(AppError::conflict(
                codes::PAYEE_NAME_TAKEN,
                "This name was used by someone who has since been renamed. Please choose a different name.",
            ));
        }
        let inserted = repository::insert(db, document).await?;
        Ok(Payee {
            key: inserted.key,
            payee_type: inserted.payee_type,
            name: inserted.name,
            phone: inserted.phone,
            usage_count: 0,
            active: inserted.active,
        })
    })
    .await
}
