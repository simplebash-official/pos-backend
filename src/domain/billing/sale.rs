//! The complete-sale request and response.

use super::invoice::CustomerRef;
use super::invoice::Invoice;
use super::invoice::PricingAdjustments;
use super::invoice::StaffRef;
use super::payment::PaymentDetails;
use super::payment::PaymentRecord;
use crate::domain::warranties::ProductWarranty;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for `POST /billing/sales`. `warnings` surfaces any sub-step
/// that failed after the invoice/payment were already durably recorded
/// (stock decrement, ticket status update, customer balance update) — see
/// D4 in the migration plan for why these never fail the whole request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompleteSaleResponse {
    /// Created invoice record.
    pub invoice: Invoice,
    /// Recorded payment transactions for this sale.
    pub payments: Vec<PaymentRecord>,
    /// Non-fatal warnings encountered during post-sale processing.
    pub warnings: Vec<String>,
}

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
    pub product_key: Option<String>,
    /// Custom item name for ad-hoc/custom retail items.
    pub name: Option<String>,
    /// Unit price in cents for ad-hoc/custom items.
    pub unit_price_cents: Option<i64>,
    /// The price tier the cashier picked for a non-serialized product line:
    /// the selling price, in cents, of the stock batches the units come from
    /// (see `Product.price_tiers`). Required for such a line, rejected when
    /// no in-stock batch sells at it. A serialized line's price comes from
    /// its serials' batch instead, and an ad-hoc line uses `unitPriceCents`.
    pub selling_price_cents: Option<i64>,
    /// Quantity of items being purchased.
    pub quantity: i64,
    /// Discount allocated directly to this line item in cents.
    pub discount_cents: i64,
    /// Line type: `"retail"`, `"repair"`, or `"print"`.
    pub source_type: String,
    /// Ticket key if checking out a completed repair or print job.
    pub source_ticket_key: Option<String>,
    /// Employee assigned to the job if applicable.
    pub assigned_employee_name: Option<String>,
    /// The specific serialized units being sold, required (exact-count
    /// validated against `quantity`) when the resolved product is
    /// serialized; omitted for non-serialized retail lines and for
    /// repair/print lines.
    pub serial_numbers: Option<Vec<String>>,
    /// A different warranty for this retail line than the product carries (coverage `none`
    /// gives it none). Needs `warranties:adjust_at_sale`; refused on repair and print lines.
    pub warranty_override: Option<ProductWarranty>,
}

/// Body for `POST /billing/sales`, grouped into sub-objects by concern
/// rather than one flat field list: `staff` (who completed it), `customer`
/// (who it was sold to, optional — a walk-in with no linked account omits
/// it entirely), `items`, `pricingAdjustments` (the invoice-level
/// discount, optional — omit for no discount), `payment` (how it was/will
/// be paid), and `shopProfileSnapshot` (frozen shop details for reprints,
/// left top-level since it isn't really "about" any one sub-concern).
/// `notes`/`warrantyTermsSnapshot`/`documentSelection` stay flat top-level
/// fields too — each is a single independent optional value with no
/// natural group to join.
///
/// `cashierId` is deliberately absent from `staff` — the completing cashier
/// is always `CurrentUser::user_id` from the bearer token, never
/// client-supplied.
///
/// `subtotalCents`/`totalCents`/`changeDueCents` are never client-supplied
/// — all three are arithmetic derived from data the server already has
/// once items are resolved: `subtotalCents` is the sum of the resolved
/// items' `totalCents`, `discountCents` is computed from
/// `pricingAdjustments` against that subtotal, `totalCents` is
/// `subtotalCents - discountCents` (there is no tax concept in this
/// system), and `changeDueCents` (when `payment.amountReceivedCents` is
/// given) is `amountReceivedCents - totalCents`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSaleRequest {
    /// Cashier staff details.
    pub staff: StaffRef,
    /// Key of the active register session on this terminal.
    pub session_key: Option<String>,
    /// Customer information or reference key.
    pub customer: Option<CustomerRef>,
    /// List of sale line items.
    pub items: Vec<CreateSaleItemRequest>,
    /// Optional sale-wide discount setting.
    pub pricing_adjustments: Option<PricingAdjustments>,
    /// Payment method and amounts.
    pub payment: PaymentDetails,
    /// Optional note printed on invoice.
    pub notes: Option<String>,
    /// Snapshot of current shop settings for receipt formatting.
    pub shop_profile_snapshot: serde_json::Value,
    /// Warranty conditions to snapshot onto the invoice.
    pub warranty_terms_snapshot: Option<String>,
    /// Document template style preference.
    pub document_selection: Option<String>,
    /// Quotation key if this sale fulfills a quotation.
    pub quotation_key: Option<String>,
}
