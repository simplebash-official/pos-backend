use super::count_credit_notes::count_credit_notes;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

/// Count of non-voided credit notes against an invoice — the guard behind
/// `service::sale::close_invoice`'s "no open credit note" rule.
pub(crate) async fn count_open_credit_notes_for_invoice(
    db: &Db,
    invoice_key: &str,
) -> AppResult<u64> {
    count_credit_notes(
        db,
        doc! { "invoice_key": invoice_key, "status": { "$ne": "voided" } },
    )
    .await
}
