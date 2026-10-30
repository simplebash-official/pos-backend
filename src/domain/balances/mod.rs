//! Pure business types for what the shop owes suppliers and what staff owe on advances.
//! No I/O, no framework types beyond serde/utoipa derives.
//!
//! Nothing here is stored as a balance. A supplier balance is the amount owed on a chosen day plus
//! purchases, less payments, since then. A staff advance is what was handed out (drawer or ledger),
//! less what was recovered from pay.

mod domain_balances;

pub use domain_balances::AdvanceGiven;
pub use domain_balances::AdvanceRecovery;
pub use domain_balances::BalanceSubject;
pub use domain_balances::CreateAdvanceRecoveryRequest;
pub use domain_balances::SetBalanceOpeningRequest;
pub use domain_balances::StaffAdvance;
pub use domain_balances::StaffAdvanceDetail;
pub use domain_balances::StaffAdvancesResponse;
pub use domain_balances::SupplierBalance;
pub use domain_balances::SupplierBalancesResponse;
