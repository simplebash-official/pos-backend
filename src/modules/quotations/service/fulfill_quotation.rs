use super::find_document::find_document;
use super::today::today;
use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::quotations::Quotation;

use crate::modules::quotations::repository;

/// Fulfills an active quotation upon converting it to a sale invoice.
pub async fn fulfill_quotation(
    db: &Db,
    id_or_key: &str,
    invoice_key: &str,
    device_id: Option<String>,
) -> AppResult<Quotation> {
    crate::core::logging::domain::tracked("quotations.fulfilled", async move {
        let existing = find_document(db, id_or_key).await?;
        let object_id = existing
            .id
            .expect("persisted quotation document must have an _id");
        match repository::fulfill(db, object_id, invoice_key, device_id).await? {
            Some(updated) => Ok(updated.into_quotation(&today())),
            None => Err(AppError::conflict(
                codes::QUOTATION_ALREADY_CANCELLED,
                "This quotation is not active and cannot be fulfilled",
            )),
        }
    })
    .await
}
