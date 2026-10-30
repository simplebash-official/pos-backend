// Retention of the cloud change log. A change older than `RETENTION_DAYS` is
// dropped and `compacted_through_seq` is raised to the newest dropped `seq`.
// Nothing is lost: a device whose cursor falls behind that point gets
// `CURSOR_EXPIRED` and rebuilds from a snapshot, which carries the current
// state of every live row (and simply omits tombstones older than the window).

mod compact_all;
mod compact_tenant;
mod retention_days;

pub use compact_all::compact_all;
pub use compact_tenant::compact_tenant;
pub(crate) use retention_days::RETENTION_DAYS;
