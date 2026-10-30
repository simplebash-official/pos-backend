//! `GET /api/sync/snapshot`: the tenant's live rows as canonical change
//! records, in apply order, paged. Used to bootstrap a new device or to recover
//! from `CURSOR_EXPIRED`. `asOfSeq` is fixed by the first page (the tenant's
//! highest seq at that moment) and repeated in every page token, so the device
//! resumes pulling from that seq afterwards; rows edited while it pages arrive
//! again through the feed and merge idempotently.

mod sync_snapshot;

pub use sync_snapshot::build_snapshot;
