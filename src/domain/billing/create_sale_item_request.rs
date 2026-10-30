use serde::Deserialize;
use utoipa::ToSchema;

/// One line of a `POST /billing/sales` request. Deliberately leaner than
/// `InvoiceItem`: `name`/`unitPriceCents` (and, for repair/print lines,
/// `assignedEmployeeName`) are only accepted here as a fallback for an
/// ad-hoc line with no key to resolve against — `sku`/`totalCents`/
/// `sourceTicketNumber` are never client-supplied at all, since they're
/// always derivable once the key resolves. `service::sale::complete_sale`
/// resolves each item from `productKey`/`sourceTicketKey` before any write;
/// an unresolvable key fails the whole request, the same way a bad
/// `customerKey` does.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSaleItemRequest {
    /// Product key for standard retail inventory items.
    #[serde(default)]
    pub product_key: Option<String>,
    /// Custom item name for ad-hoc/custom retail items.
    #[serde(default)]
    pub name: Option<String>,
    /// Unit price in cents for ad-hoc/custom items.
    #[serde(default)]
    pub unit_price_cents: Option<i64>,
    /// The price tier the cashier picked for a non-serialized product line:
    /// the selling price, in cents, of the stock batches the units come from
    /// (see `Product.price_tiers`). Required for such a line, rejected when
    /// no in-stock batch sells at it. A serialized line's price comes from
    /// its serials' batch instead, and an ad-hoc line uses `unitPriceCents`.
    #[serde(default)]
    pub selling_price_cents: Option<i64>,
    /// Quantity of items being purchased.
    pub quantity: i64,
    /// Discount allocated directly to this line item in cents.
    pub discount_cents: i64,
    /// Line type: `"retail"`, `"repair"`, or `"print"`.
    pub source_type: String,
    /// Ticket key if checking out a completed repair or print job.
    #[serde(default)]
    pub source_ticket_key: Option<String>,
    /// Employee assigned to the job if applicable.
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
    /// The specific serialized units being sold, required (exact-count
    /// validated against `quantity`) when the resolved product is
    /// serialized; omitted for non-serialized retail lines and for
    /// repair/print lines.
    #[serde(default)]
    pub serial_numbers: Option<Vec<String>>,
}
