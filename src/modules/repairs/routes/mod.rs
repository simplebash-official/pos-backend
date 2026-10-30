//! HTTP layer only — same shape as `modules::customers::routes`. Reads
//! require only a valid token; writes additionally require `REPAIRS_WRITE`
//! (first route in the codebase to actually enforce that permission — it was
//! defined as a placeholder in `core::constants::permissions` until now).

mod repairs_routes;

pub use repairs_routes::router;
