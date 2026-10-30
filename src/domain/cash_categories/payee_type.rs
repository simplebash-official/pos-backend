use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Who a payee is. Suppliers and staff live in their own modules and are listed
/// here by reference; only `Vendor` and `Other` rows are stored in `payees`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayeeType {
    Supplier,
    Staff,
    /// Landlord or utility company, added on purpose by a manager.
    Vendor,
    /// A name typed once while paying a small expense, saved for next time.
    Other,
}

impl PayeeType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            PayeeType::Supplier => "supplier",
            PayeeType::Staff => "staff",
            PayeeType::Vendor => "vendor",
            PayeeType::Other => "other",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supplier" => Some(PayeeType::Supplier),
            "staff" => Some(PayeeType::Staff),
            "vendor" => Some(PayeeType::Vendor),
            "other" => Some(PayeeType::Other),
            _ => None,
        }
    }
}
