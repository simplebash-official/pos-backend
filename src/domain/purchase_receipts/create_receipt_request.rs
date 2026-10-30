use super::charges_payment::ChargesPayment;
use super::receipt_approval::ReceiptApproval;
use super::receipt_charge_request::ReceiptChargeRequest;
use super::receipt_line_request::ReceiptLineRequest;
use super::receipt_payment::ReceiptPayment;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /purchase-receipts`. Recording a receipt brings every line into stock as its
/// own batch, in one all-or-nothing save.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateReceiptRequest {
    /// Foreign key of the supplier.
    pub supplier_key: String,
    /// The supplier's invoice or delivery-note number.
    #[serde(default)]
    pub reference_no: Option<String>,
    /// Day the stock was received.
    pub date: DateTime<Utc>,
    /// Optional remarks.
    #[serde(default)]
    pub notes: Option<String>,
    /// At least one line; a product may appear on only one line.
    pub lines: Vec<ReceiptLineRequest>,
    /// What is paid as the stock is received. Absent means the whole receipt is on credit.
    #[serde(default)]
    pub payment: Option<ReceiptPayment>,
    /// Extra charges: shipping, duty, handling, insurance, a discount. They go into the cost of the
    /// stock; the ones the supplier bills also change what is owed to the supplier.
    #[serde(default)]
    pub charges: Vec<ReceiptChargeRequest>,
    /// How the third-party charges are paid. Required when any charge is `third_party`, refused
    /// otherwise.
    #[serde(default)]
    pub charges_payment: Option<ChargesPayment>,
    /// Manager or owner approval for the drawer cash this request takes out.
    #[serde(default)]
    pub approval: Option<ReceiptApproval>,
    /// Set after a `DUPLICATE_RECEIPT` warning to record the receipt anyway.
    #[serde(default)]
    pub confirm_duplicate: bool,
}
