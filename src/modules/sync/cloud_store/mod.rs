// Storage layer of the cloud (MongoDB) side of sync v2: the tenant-scoped
// collections behind push/pull/snapshot and the small pieces of state around
// them (per-tenant sequence, device registry, replayable batch acks,
// conflicts). Every accessor goes through the scoped handle, so a call inside
// `core::tenancy::with_tenant` can only ever see its own tenant's rows.
//
// It deliberately owns no business rules: what a change means lives in
// `apply_mongo`/`push`, and ordering lives in the change-stream consumer.

mod allocate_seq;
mod batches;
mod changes;
mod coll;
mod compacted_through;
mod conflicts;
mod current_seq;
mod devices;
mod ensure_device_active;
mod ensure_device_app_current;
mod find_batch_acks;
mod meta;
mod meta_i64;
mod min_desktop_app_version;
mod parse_app_version;
mod record_conflicts;
mod record_device_app_version;
mod record_hard_deletes;
mod set_compacted_through;
mod store_batch_acks;
#[cfg(test)]
mod tests;
mod tombstones;
mod touch_device;
mod update_required;

pub(crate) use allocate_seq::allocate_seq;

pub(crate) use changes::CHANGES;
pub(crate) use coll::coll;
pub(crate) use compacted_through::compacted_through;
pub(crate) use conflicts::CONFLICTS;
pub(crate) use current_seq::current_seq;
pub(crate) use devices::DEVICES;
pub(crate) use ensure_device_active::ensure_device_active;
pub(crate) use ensure_device_app_current::ensure_device_app_current;
pub(crate) use find_batch_acks::find_batch_acks;
pub(crate) use meta::META;

pub(crate) use record_conflicts::record_conflicts;
pub(crate) use record_device_app_version::record_device_app_version;
pub(crate) use record_hard_deletes::record_hard_deletes;
pub(crate) use set_compacted_through::set_compacted_through;
pub(crate) use store_batch_acks::store_batch_acks;
pub(crate) use tombstones::TOMBSTONES;
pub(crate) use touch_device::touch_device;
