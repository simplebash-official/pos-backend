//! Storage layer of the cloud (MongoDB) side of sync v2: the tenant-scoped
//! collections behind push/pull/snapshot and the small pieces of state around
//! them (per-tenant sequence, device registry, replayable batch acks,
//! conflicts). Every accessor goes through the scoped handle, so a call inside
//! `core::tenancy::with_tenant` can only ever see its own tenant's rows.
//!
//! It deliberately owns no business rules: what a change means lives in
//! `apply_mongo`/`push`, and ordering lives in the change-stream consumer.

mod sync_cloud_store;
#[cfg(test)]
mod tests;

pub(crate) use sync_cloud_store::allocate_seq;

pub(crate) use sync_cloud_store::CHANGES;
pub(crate) use sync_cloud_store::CONFLICTS;
pub(crate) use sync_cloud_store::DEVICES;
pub(crate) use sync_cloud_store::META;
pub(crate) use sync_cloud_store::coll;
pub(crate) use sync_cloud_store::compacted_through;
pub(crate) use sync_cloud_store::current_seq;
pub(crate) use sync_cloud_store::ensure_device_active;
pub(crate) use sync_cloud_store::ensure_device_app_current;
pub(crate) use sync_cloud_store::find_batch_acks;

pub(crate) use sync_cloud_store::TOMBSTONES;
pub(crate) use sync_cloud_store::record_conflicts;
pub(crate) use sync_cloud_store::record_device_app_version;
pub(crate) use sync_cloud_store::record_hard_deletes;
pub(crate) use sync_cloud_store::set_compacted_through;
pub(crate) use sync_cloud_store::store_batch_acks;
pub(crate) use sync_cloud_store::touch_device;
