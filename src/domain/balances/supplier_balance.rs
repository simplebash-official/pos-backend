use serde::Serialize;
use utoipa::ToSchema;

/// What the shop owes one supplier. A supplier has no balance until an opening balance is set,
/// because stock may have been paid for before payments were tracked here.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBalance {
    pub supplier_key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_balance_cents: Option<i64>,
    /// `YYYY-MM-DD`: purchases and payments from this day on count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_date: Option<String>,
    /// Purchases (quantity x unit cost) since the opening day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchased_cents: Option<i64>,
    /// Paid from the drawer, the bank, a card or online since the opening day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_cents: Option<i64>,
    /// Taken off by supplier credits (short or damaged goods, price differences) since the opening day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credited_cents: Option<i64>,
    /// What is owed; negative when the shop has paid ahead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_cents: Option<i64>,
}
