use super::payee_type::PayeeType;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One row of the shared payee directory.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Payee {
    /// `sup_…` / `emp_…` for suppliers and staff, `pye_…` for stored payees.
    pub key: String,
    pub payee_type: PayeeType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// How many cash movements name this payee; the combobox puts frequent ones first.
    pub usage_count: i64,
    /// Hidden payees are not offered when paying someone new; their history stays. Suppliers and
    /// staff are always active here: their own pages decide who is listed.
    pub active: bool,
}
