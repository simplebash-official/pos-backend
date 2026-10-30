//! Replay protection for write requests that carry an `Idempotency-Key`.

mod handle_idempotency;
mod middleware_idempotency;

pub use handle_idempotency::handle_idempotency;
pub use middleware_idempotency::IdempotencyDocument;
pub use middleware_idempotency::MAX_IDEMPOTENT_BODY_BYTES;
