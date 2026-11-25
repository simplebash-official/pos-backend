//! Which synced resources the catch-up covers.

use crate::modules::sync::resources::{self, ResourceSpec};

/// The resources earlier builds had no sync hydrator for.
const LEDGER_RESOURCES: [&str; 9] = [
    "payees",
    "cashCategories",
    "recurringExpenses",
    "incomes",
    "accountOpenings",
    "balanceOpenings",
    "advanceRecoveries",
    "attachments",
    "cashSettings",
];

/// The specs of those resources.
pub(super) fn ledger_specs() -> impl Iterator<Item = &'static ResourceSpec> {
    LEDGER_RESOURCES
        .into_iter()
        .map(|name| resources::spec(name).expect("every ledger resource is a synced resource"))
}
