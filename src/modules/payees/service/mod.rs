// Business rules for the payee directory: one list over suppliers, staff and stored
// payees, name checks, and resolving a payee key from a cash movement.

mod clean_name;
mod clean_phone;
mod create_payee;
mod ensure_other_payee;
mod list_payees;
mod new_document;
mod resolve_payee;
mod resolved_payee;
mod update_payee;

pub use create_payee::create_payee;
pub(crate) use ensure_other_payee::ensure_other_payee;
pub use list_payees::list_payees;
pub(crate) use resolve_payee::resolve_payee;
pub use update_payee::update_payee;
