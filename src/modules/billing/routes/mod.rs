//! HTTP layer for billing: sale completion, invoice/payment reads, payment
//! recording, cancellation, and on-demand document (PDF) fetching. The
//! module-status stub at `GET /` is unrelated to any of that and stays as
//! the intentionally-public placeholder it always was (see `app.rs`'s
//! `PUBLIC_ROUTES` note) — `/invoices` is the real "list" entry point, not
//! `/`, since billing's root has no natural single-collection meaning the
//! way `suppliers`'/`customers`' root does.

mod billing_routes;

pub use billing_routes::router;
