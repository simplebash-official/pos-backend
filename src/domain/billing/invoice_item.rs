use crate::domain::inventory::BatchAllocation;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One line of a sale, embedded in both the complete-sale request and the
/// persisted/returned `Invoice`. `source_ticket_key` (the repair/print-job's
/// Mongo-independent `key`, e.g. `rep_...`) is what `complete_sale` uses to
/// mark a ticket delivered — `source_ticket_number` (`REP-000001`) is
/// display-only, shown on receipts/invoices and in the cart.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceItem {
    /// Foreign key referencing the catalog product, if this is a retail item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    /// Display name or description of the sold item or service.
    pub name: String,
    /// Stock Keeping Unit (SKU) barcode identifier, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    /// Price per unit in cents.
    pub unit_price_cents: i64,
    /// Number of units purchased.
    pub quantity: i64,
    /// Discount amount given on this specific item in cents.
    pub discount_cents: i64,
    /// Total price for this line item in cents after discount.
    pub total_cents: i64,
    /// Snapshot of the unit cost at sale time, in cents. For a retail line
    /// this is the weighted cost of the stock batches the units came from
    /// (see `batch_allocations`);
    /// for a repair/print line it is the linked ticket's `material_cost_cents`
    /// divided across the line quantity. `None` on an ad-hoc line with no
    /// product/ticket to resolve against, and on every invoice created before
    /// this field existed — treat a missing value as "cost unknown", never as
    /// zero, in any margin calculation that must not silently understate cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_cost_cents: Option<i64>,
    /// Source category: `"retail"`, `"repair"`, or `"print"`.
    pub source_type: String,
    /// Key of linked repair or print job ticket, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_key: Option<String>,
    /// Formatted ticket number of linked repair or print job (e.g. REP-000001).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_number: Option<String>,
    /// Display name of technician or operator who handled this ticket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    /// Quantity of items already returned from this line item.
    #[serde(default)]
    pub returned_quantity: i64,
    /// The specific serialized units sold on this line, if the product is
    /// serialized (see `Product.is_serialized`) — one entry per unit,
    /// `quantity` long. Empty for a non-serialized retail line and for
    /// repair/print lines.
    #[serde(default)]
    pub serial_numbers: Vec<String>,
    /// Which stock batches a retail product line's units came out of (one
    /// line at one price can span several batches, oldest first). A void or
    /// a return puts units back into exactly these batches. Empty for
    /// repair/print and ad-hoc lines, which move no stock.
    #[serde(default)]
    pub batch_allocations: Vec<BatchAllocation>,
}
