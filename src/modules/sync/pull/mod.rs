//! `GET /api/sync/pull`: the ordered change feed a device reads. The cursor is
//! the tenant's integer `seq` (assigned by the single change-stream consumer),
//! not a timestamp, so device clocks never affect what a puller sees.
//!
//! A device does not receive its own writes back: changes whose
//! `origin_device_id` is the caller are skipped, but the cursor still advances
//! past them, so a device that only ever pushes does not re-scan the same range.

mod sync_pull;

pub use sync_pull::pull_since;
