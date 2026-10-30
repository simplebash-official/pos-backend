//! Business rules for the money accounts: where each starts, and what each holds now, worked out
//! from sales payments, income, expenses and the cash moved to and from the bank.

mod accounts_service;
#[cfg(test)]
mod tests;

#[cfg(test)]
use accounts_service::balance_of;
pub use accounts_service::list_accounts;
#[cfg(test)]
use accounts_service::opening_key;
pub use accounts_service::set_opening;
