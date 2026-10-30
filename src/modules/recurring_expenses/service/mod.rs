//! Business rules for regular bills: what a template may say, how a bill stands for a month, and
//! the "already paid" check used before a rent payment is recorded.

mod recurring_expenses_service;
#[cfg(test)]
mod tests;

#[cfg(test)]
use recurring_expenses_service::check_payee;
#[cfg(test)]
use recurring_expenses_service::check_schedule;
pub use recurring_expenses_service::create_template;
#[cfg(test)]
use recurring_expenses_service::due_date_in;
pub use recurring_expenses_service::list_bills;
#[cfg(test)]
use recurring_expenses_service::month_string;
#[cfg(test)]
use recurring_expenses_service::parse_month;
pub use recurring_expenses_service::period_status;
#[cfg(test)]
use recurring_expenses_service::previous_month;
#[cfg(test)]
use recurring_expenses_service::status_for;
pub use recurring_expenses_service::update_template;
