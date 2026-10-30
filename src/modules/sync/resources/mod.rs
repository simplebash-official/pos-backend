//! Single source of truth for which resources sync, how each merges, which
//! columns are derived (never taken from a payload) and the order changes are
//! applied in. Shared by the device (SQLite) and cloud (Mongo) sides so both
//! agree by construction. The older `SYNCABLE` list in `service.rs` keeps
//! serving the legacy `/sync/changes` route and is intentionally left alone.

mod sync_resources;
mod sync_resources_group;
#[cfg(test)]
mod tests;

pub use sync_resources::SYNC_RESOURCES;
pub use sync_resources_group::MergeClass;
pub use sync_resources_group::Phase;
pub use sync_resources_group::ResourceSpec;
pub use sync_resources_group::label_column;
pub use sync_resources_group::ordered;
pub use sync_resources_group::secret_fields;
pub use sync_resources_group::snake_to_camel;
pub use sync_resources_group::spec;
pub use sync_resources_group::without_secrets;
