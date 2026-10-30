//! `POST /api/sync/push`: a device sends its outbox changes; the cloud merges
//! each one with the pure engine (`core::sync_merge::decide`), writes the winners
//! through `apply_mongo`, recomputes derived fields once for the whole batch and
//! answers with a per-change ack. Retrying a batch is safe twice over: the
//! recorded acks of a `(deviceId, batchId)` are replayed, and re-applying an
//! identical change decides `Duplicate` anyway.
//!
//! The push path never writes `sync_changes`. The change-stream consumer does
//! (`cloud_capture`), so web-originated and device-originated writes reach the
//! pull feed through the same single, ordered path.

mod sync_push;

pub use sync_push::push_batch;
