use super::item_condition::ItemCondition;
use super::item_disposition::ItemDisposition;
use super::return_reason::ReturnReason;
use crate::domain::inventory::BatchAllocation;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A line item in a credit note representing a returned product or service.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreditNoteItem {
    /// Foreign key referencing the catalog product, if this is a retail item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    /// Display name or description of the returned item.
    pub name: String,
    /// Stock Keeping Unit (SKU) barcode identifier, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    /// Number of units returned.
    pub quantity: i64,
    /// Price per unit in cents refunded.
    pub unit_price_cents: i64,
    /// Total price for this line item in cents refunded.
    pub total_cents: i64,
    /// Reason why the item was returned.
    pub reason: ReturnReason,
    /// The item's physical/sellable condition.
    pub condition: ItemCondition,
    /// What happens to the item — required iff `condition == Damaged`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<ItemDisposition>,
    /// The specific serialized unit returned, if the original line was for
    /// a serialized product.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Whether the returned serial was still within its warranty period at
    /// the time of return. Response-only, computed from the matched
    /// `ProductSerial`; absent for non-serialized items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within_warranty: Option<bool>,
    /// Optional notes or defect details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Source category: `"retail"`, `"repair"`, or `"print"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Key of linked repair or print job ticket, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_key: Option<String>,
    /// Formatted ticket number of linked repair or print job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_number: Option<String>,
    /// The stock batches a product line's units belong back in: the sold
    /// line's own batches (newest first) for a receipt return, the newest
    /// batch at the picked price for a no-receipt return. Units are only put
    /// back for a restockable condition; voiding the credit note takes them
    /// out of these same batches again.
    #[serde(default)]
    pub batch_allocations: Vec<BatchAllocation>,
}
