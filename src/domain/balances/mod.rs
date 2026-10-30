// Pure business types for what the shop owes suppliers and what staff owe on advances.
// No I/O, no framework types beyond serde/utoipa derives.
//
// Nothing here is stored as a balance. A supplier balance is the amount owed on a chosen day plus
// purchases, less payments, since then. A staff advance is what was handed out (drawer or ledger),
// less what was recovered from pay.

mod advance_given;
mod advance_recovery;
mod balance_subject;
mod create_advance_recovery_request;
mod set_balance_opening_request;
mod staff_advance;
mod staff_advance_detail;
mod staff_advances_response;
mod supplier_balance;
mod supplier_balances_response;

pub use advance_given::AdvanceGiven;
pub use advance_recovery::AdvanceRecovery;
pub use balance_subject::BalanceSubject;
pub use create_advance_recovery_request::CreateAdvanceRecoveryRequest;
pub use set_balance_opening_request::SetBalanceOpeningRequest;
pub use staff_advance::StaffAdvance;
pub use staff_advance_detail::StaffAdvanceDetail;
pub use staff_advances_response::StaffAdvancesResponse;
pub use supplier_balance::SupplierBalance;
pub use supplier_balances_response::SupplierBalancesResponse;
