use super::receipt_charge::ReceiptCharge;
use super::receipt_settlement::ReceiptSettlement;
use super::receipt_status::ReceiptStatus;

use crate::domain::purchases::{Purchase, SupplierSummary};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A receipt as returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseReceipt {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`prc_...`).
    pub key: String,
    /// Running number shown to people (`PUR-000001`).
    pub receipt_number: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// The supplier's invoice or delivery-note number, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    /// Day the stock was received.
    pub date: DateTime<Utc>,
    /// Optional remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Enriched supplier details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<SupplierSummary>,
    /// The products received, each with its own cost and selling price.
    pub lines: Vec<Purchase>,
    /// What the lines cost, before any extra charge: the sum of the lines' totals.
    pub goods_cents: i64,
    /// Extra charges the supplier put on the bill, discounts taken off (signed). Adds to what is owed.
    pub extra_charges_cents: i64,
    /// Extra charges paid to someone other than the supplier (courier, agent). Not owed to the supplier.
    pub paid_to_others_cents: i64,
    /// What the supplier's bill comes to: the goods plus the extra charges it carries.
    pub total_cents: i64,
    /// The extra charges on this receipt, oldest first.
    pub charges: Vec<ReceiptCharge>,
    /// Taken off the bill by supplier credits (short or damaged goods, price differences).
    pub credited_cents: i64,
    /// Paid against the bill, reversed payments left out.
    pub paid_cents: i64,
    /// What is still owed: the total less the credits and the payments.
    pub due_cents: i64,
    pub status: ReceiptStatus,
    /// The payments that settled this receipt, oldest first.
    pub settlements: Vec<ReceiptSettlement>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    pub version: i64,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
