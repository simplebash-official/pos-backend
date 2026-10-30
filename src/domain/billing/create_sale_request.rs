use super::create_sale_item_request::CreateSaleItemRequest;
use super::customer_ref::CustomerRef;
use super::payment_details::PaymentDetails;
use super::pricing_adjustments::PricingAdjustments;
use super::staff_ref::StaffRef;

use serde::Deserialize;
use utoipa::ToSchema;

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
    #[serde(default)]
    pub session_key: Option<String>,
    /// Customer information or reference key.
    #[serde(default)]
    pub customer: Option<CustomerRef>,
    /// List of sale line items.
    pub items: Vec<CreateSaleItemRequest>,
    /// Optional sale-wide discount setting.
    #[serde(default)]
    pub pricing_adjustments: Option<PricingAdjustments>,
    /// Payment method and amounts.
    pub payment: PaymentDetails,
    /// Optional note printed on invoice.
    #[serde(default)]
    pub notes: Option<String>,
    /// Snapshot of current shop settings for receipt formatting.
    pub shop_profile_snapshot: serde_json::Value,
    /// Warranty conditions to snapshot onto the invoice.
    #[serde(default)]
    pub warranty_terms_snapshot: Option<String>,
    /// Document template style preference.
    #[serde(default)]
    pub document_selection: Option<String>,
    /// Quotation key if this sale fulfills a quotation.
    #[serde(default)]
    pub quotation_key: Option<String>,
}
