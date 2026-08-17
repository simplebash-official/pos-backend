// Sale completion, invoices, and payments — the centerpiece of the
// billing-backend migration. Layered model (BSON shapes) -> repository
// (Mongo only) -> service (business rules/orchestration) -> routes (HTTP),
// same shape as `modules::customers`/`modules::suppliers`.
//
// `service::sale::complete_sale` is the one place a sale is created; it
// also calls into `repairs::service::mark_delivered`,
// `print_jobs::service::mark_delivered`, `customers::service::
// apply_financial_delta`, and `inventory::service::stock::
// apply_stock_delta` — see that function's doc comment (D4 in the
// migration plan) for why none of those sub-steps can fail the whole
// request once the invoice itself is durably inserted.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
