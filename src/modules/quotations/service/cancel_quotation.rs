use super::find_document::find_document;
use super::today::today;
use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::quotations::Quotation;

use crate::modules::quotations::repository;

/// Cancels an active quotation. A cancelled quotation stays printable (the
/// PDF is stamped CANCELLED) so a paper copy still in a customer's hands can
/// be checked against the record.
pub async fn cancel_quotation(
    db: &Db,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<Quotation> {
    crate::core::logging::domain::tracked("quotations.cancelled", async move {
        let existing = find_document(db, id_or_key).await?;
        let object_id = existing
            .id
            .expect("persisted quotation document must have an _id");
        match repository::cancel(db, object_id, device_id).await? {
            Some(updated) => Ok(updated.into_quotation(&today())),
            None => Err(AppError::conflict(
                codes::QUOTATION_ALREADY_CANCELLED,
                "This quotation is already cancelled",
            )),
        }
    })
    .await
}
