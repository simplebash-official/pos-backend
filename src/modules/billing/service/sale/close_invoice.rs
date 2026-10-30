use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::billing::{Invoice, InvoiceStatus};

use crate::modules::billing::repository;

use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Marks a fully paid invoice `Closed` — a manual, explicit "done, nothing
/// more will happen to this invoice" action (never auto-computed). Blocked
/// unless the invoice is `Paid` and has no open (non-voided) credit notes
/// against it; re-queries `credit_notes` live for that guard rather than
/// trusting `Invoice.credit_note_count` alone.
pub async fn close_invoice(db: &Db, id_or_key: &str, closed_by: String) -> AppResult<Invoice> {
    crate::core::logging::domain::tracked("billing.invoice_closed", async move {
        let existing = repository::find_invoice_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        if existing.status != InvoiceStatus::Paid {
            return Err(AppError::conflict(
                codes::INVOICE_NOT_CLOSABLE,
                "Only a fully paid invoice can be closed",
            ));
        }

        let open_credit_notes =
            repository::credit_notes::count_open_credit_notes_for_invoice(db, &existing.key)
                .await?;
        if open_credit_notes > 0 {
            return Err(AppError::conflict(
                codes::INVOICE_NOT_CLOSABLE,
                "This invoice has an open credit note — void or resolve it before closing",
            ));
        }

        let object_id = existing
            .id
            .expect("persisted invoice document must have an _id");
        let set_doc = doc! {
            "status": InvoiceStatus::Closed.as_str(),
            "closed_at": BsonDateTime::now(),
            "closed_by": &closed_by,
            "updated_at": BsonDateTime::now(),
        };

        let updated = repository::update_invoice_status(db, object_id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        Ok(updated.into_invoice())
    })
    .await
}
