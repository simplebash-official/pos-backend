use super::calc_exchange_line::CalcExchangeLine;
use super::calc_no_receipt_line::CalcNoReceiptLine;
use super::calc_returned_line::CalcReturnedLine;

use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCreditNoteRequest {
    /// The invoice the return is against; absent (with `noReceipt`) for a return with no receipt.
    pub invoice_key: Option<String>,
    pub returned: Vec<CalcReturnedLine>,
    pub no_receipt: Option<CalcNoReceiptLine>,
    pub exchanged: Vec<CalcExchangeLine>,
}
