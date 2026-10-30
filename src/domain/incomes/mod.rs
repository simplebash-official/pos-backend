//! Pure business types for the income ledger: money the shop receives that is not a sale
//! (scrap, commission, supplier refunds, interest, rent from a sub-tenant). Sales income is never
//! entered; it is worked out from invoices. No I/O, no framework types beyond serde/utoipa derives.
//!
//! Two sources feed one list, like expenses. Cash put into the drawer is entered once, as an
//! `add_cash` movement under an income category, and is read into this list, never copied.
//! Money received into the bank, a card or online is entered here as an `Income`.

mod domain_incomes;

pub use domain_incomes::CreateIncomeRequest;
pub use domain_incomes::IncomeListQuery;
pub use domain_incomes::IncomeListResponse;
pub use domain_incomes::IncomeRow;
pub use domain_incomes::IncomeSource;
pub use domain_incomes::IncomeStats;
pub use domain_incomes::ReceivedInto;
