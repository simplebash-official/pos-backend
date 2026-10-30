//! Business rules for the payee directory: one list over suppliers, staff and stored
//! payees, name checks, and resolving a payee key from a cash movement.

mod payees_service;

pub use payees_service::create_payee;
pub(crate) use payees_service::ensure_other_payee;
pub use payees_service::list_payees;
pub(crate) use payees_service::resolve_payee;
pub use payees_service::update_payee;
