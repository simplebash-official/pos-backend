//! HTTP layer for the live calculations: each handler checks the same permission as the write it
//! previews, makes one call into the pure engine and wraps the reply. Nothing is stored, so none of
//! these count as writes (no idempotency key, no sync wake-up; see `app.rs`).

mod calc_routes;

pub use calc_routes::router;
