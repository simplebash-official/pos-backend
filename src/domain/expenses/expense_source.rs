use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whether a row of the list came from the ledger or from the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseSource {
    Ledger,
    Drawer,
}
