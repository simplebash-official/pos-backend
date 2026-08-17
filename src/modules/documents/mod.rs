// Generated PDF bookkeeping (invoices/receipts rendered via the sibling
// document-server). Unlike every other module in this crate, this one has
// no `routes.rs` and is never `.nest()`ed in `app.rs` — it has no HTTP
// surface of its own. `modules::billing` (the only intended caller) reaches
// into `service::get_or_render` the same way it reaches into
// `customers::service::apply_financial_delta`: a private cross-module hook,
// not a public REST resource. `repository` stays private so nothing outside
// this module tree can touch the `generated_documents` Mongo collection
// directly.
mod model;
mod repository;
pub mod service;
