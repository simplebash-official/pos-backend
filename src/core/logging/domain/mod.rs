//! Business-event logging for service functions. Wrapping a mutating service
//! body in `tracked` records one `category=domain` line per call: the
//! outcome, how long it took, and the identifying `key` of what was created
//! or changed (read from the returned value). The full request/response
//! bodies are already in the `http` lines of the same `request_id`; this is
//! the business-level summary ("sale.completed inv_… in 42 ms").
//!
//! ```ignore
//! pub async fn complete_sale(state: &AppState, input: SaleInput) -> AppResult<Invoice> {
//!     tracked("billing.sale_completed", async move {
//!         /* existing body */
//!     })
//!     .await
//! }
//! ```

mod logging_domain;
#[cfg(test)]
mod tests;

pub use logging_domain::tracked;
