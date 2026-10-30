// Who made the write that is running right now, for cloud sync. Every document
// in a synced collection carries `updated_by_device`; the change-stream
// consumer copies it into `sync_changes.origin_device_id`, and a device's pull
// skips its own echoes by that value. A write that left the previous writer's
// id in place would therefore be hidden from that device forever, so the stamp
// is applied centrally (`clients::tenant_db::ScopedCollection`) instead of
// trusting every repository to remember it.
//
// Nothing here does I/O; the request middleware (`core::middleware::tenant`)
// puts the origin in scope, and `tenant_db` applies the rewrites below.

mod cloud_origin;
mod current_origin;
mod current_origin_fn;
mod is_synced_collection;
mod origin_field;
mod origin_for;
mod stamp_document;
mod stamp_update;
mod synced_collections;
#[cfg(test)]
mod tests;
mod web_prefix;
mod with_origin;

pub use cloud_origin::CLOUD_ORIGIN;
pub use current_origin_fn::current_origin;
pub use is_synced_collection::is_synced_collection;
pub use origin_field::ORIGIN_FIELD;
pub use origin_for::origin_for;
pub use stamp_document::stamp_document;
pub use stamp_update::stamp_update;
pub use synced_collections::SYNCED_COLLECTIONS;
pub use web_prefix::WEB_PREFIX;
pub use with_origin::with_origin;
