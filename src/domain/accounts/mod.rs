//! Pure business types for the shop's money accounts: the cash drawer, the bank, cards and online.
//! No I/O, no framework types beyond serde/utoipa derives.
//!
//! A balance is never stored. The drawer is the running cash of its terminals; the bank, card and
//! online accounts start from an opening balance the owner sets once and are worked out from what
//! moved on and after that day.

mod domain_accounts;

pub use domain_accounts::AccountActivity;
pub use domain_accounts::AccountKind;
pub use domain_accounts::AccountSummary;
pub use domain_accounts::AccountsResponse;
pub use domain_accounts::SetAccountOpeningRequest;
