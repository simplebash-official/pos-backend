//! Reads the device outbox as canonical change records: each queued row is
//! resolved to its CURRENT state (the outbox only remembers that something
//! changed), hydrated to the same DTO the REST endpoints return, and stamped
//! with the merge metadata the triggers recorded (skew-corrected, monotonic
//! time and the writing device).

mod sync_outbox;

pub use sync_outbox::list_outbox;
pub use sync_outbox::list_pending;
pub(crate) use sync_outbox::ms_to_datetime;
