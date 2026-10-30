use super::item_condition::ItemCondition;
use super::item_disposition::ItemDisposition;
use super::return_reason::ReturnReason;

use serde::Deserialize;
use utoipa::ToSchema;

fn default_return_reason() -> ReturnReason {
    ReturnReason::Other
}

/// One item in a `POST /billing/credit-notes` request.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCreditNoteItemRequest {
    /// Product key for standard retail inventory items.
    #[serde(
        default,
        alias = "productId",
        alias = "product_id",
        alias = "product_key"
    )]
    pub product_key: Option<String>,
    /// Item name matching the invoice line if no product key (required for
    /// a no-receipt line, which has no invoice line to match against).
    #[serde(default)]
    pub name: Option<String>,
    /// Quantity of items being returned.
    pub quantity: i64,
    /// Reason for return.
    #[serde(default = "default_return_reason")]
    pub reason: ReturnReason,
    /// The item's physical/sellable condition.
    #[serde(alias = "itemCondition")]
    pub condition: ItemCondition,
    /// What happens to the item — required iff `condition == "damaged"`.
    #[serde(default)]
    pub disposition: Option<ItemDisposition>,
    /// The specific serialized unit being returned, if applicable.
    #[serde(default)]
    pub serial_number: Option<String>,
    /// Optional notes or reason description.
    #[serde(default)]
    pub notes: Option<String>,
    /// Unit price in cents for custom/ad-hoc lines.
    #[serde(default, alias = "unit_price_cents")]
    pub unit_price_cents: Option<i64>,
    /// For a no-receipt product line: the price the item was sold at, picked
    /// from the product's prices. The return is valued at it and restocked
    /// into the newest batch sold at it.
    #[serde(default)]
    pub selling_price_cents: Option<i64>,
    /// Source ticket key if returning a repair/print line.
    #[serde(default, alias = "source_ticket_key")]
    pub source_ticket_key: Option<String>,
}
