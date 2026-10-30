// Pure business types for the income ledger: money the shop receives that is not a sale
// (scrap, commission, supplier refunds, interest, rent from a sub-tenant). Sales income is never
// entered; it is worked out from invoices. No I/O, no framework types beyond serde/utoipa derives.
//
// Two sources feed one list, like expenses. Cash put into the drawer is entered once, as an
// `add_cash` movement under an income category, and is read into this list, never copied.
// Money received into the bank, a card or online is entered here as an `Income`.

mod create_income_request;
mod income_list_query;
mod income_list_response;
mod income_row;
mod income_source;
mod income_stats;
mod received_into;

pub use create_income_request::CreateIncomeRequest;
pub use income_list_query::IncomeListQuery;
pub use income_list_response::IncomeListResponse;
pub use income_row::IncomeRow;
pub use income_source::IncomeSource;
pub use income_stats::IncomeStats;
pub use received_into::ReceivedInto;
