//! Who made the write that is running right now, for cloud sync. Every document
//! in a synced collection carries `updated_by_device`; the change-stream
//! consumer copies it into `sync_changes.origin_device_id`, and a device's pull
//! skips its own echoes by that value. A write that left the previous writer's
//! id in place would therefore be hidden from that device forever, so the stamp
//! is applied centrally (`clients::tenant_db::ScopedCollection`) instead of
//! trusting every repository to remember it.
//!
//! Nothing here does I/O; the request middleware (`core::middleware::tenant`)
//! puts the origin in scope, and `tenant_db` applies the rewrites below.

mod core_sync_origin;
#[cfg(test)]
mod tests;

pub use core_sync_origin::CLOUD_ORIGIN;
pub use core_sync_origin::ORIGIN_FIELD;
pub use core_sync_origin::SYNCED_COLLECTIONS;
pub use core_sync_origin::WEB_PREFIX;
pub use core_sync_origin::current_origin;
pub use core_sync_origin::is_synced_collection;
pub use core_sync_origin::origin_for;
pub use core_sync_origin::stamp_document;
pub use core_sync_origin::stamp_update;
pub use core_sync_origin::with_origin;
