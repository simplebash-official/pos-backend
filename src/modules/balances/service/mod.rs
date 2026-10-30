//! Business rules for supplier balances and staff advances: where each starts, what has moved since,
//! and recording the part of an advance taken back from pay. Nothing here is a stored balance.

mod balances_service;
#[cfg(test)]
mod tests;

pub use balances_service::get_staff_advance;
pub use balances_service::list_staff_advances;
pub use balances_service::list_supplier_balances;
#[cfg(test)]
use balances_service::opening_key;
#[cfg(test)]
use balances_service::owed;
pub use balances_service::recover_advance;
pub use balances_service::set_opening;
