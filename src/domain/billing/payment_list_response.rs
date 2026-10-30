use super::payment_record::PaymentRecord;

use serde::Serialize;
use utoipa::ToSchema;

/// Response payload containing all payments recorded against an invoice.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentListResponse {
    /// List of payments recorded against the invoice.
    pub payments: Vec<PaymentRecord>,
}
