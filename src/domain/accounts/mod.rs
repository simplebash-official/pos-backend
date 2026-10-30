// Pure business types for the shop's money accounts: the cash drawer, the bank, cards and online.
// No I/O, no framework types beyond serde/utoipa derives.
//
// A balance is never stored. The drawer is the running cash of its terminals; the bank, card and
// online accounts start from an opening balance the owner sets once and are worked out from what
// moved on and after that day.

mod account_activity;
mod account_kind;
mod account_summary;
mod accounts_response;
mod set_account_opening_request;

pub use account_activity::AccountActivity;
pub use account_kind::AccountKind;
pub use account_summary::AccountSummary;
pub use accounts_response::AccountsResponse;
pub use set_account_opening_request::SetAccountOpeningRequest;
