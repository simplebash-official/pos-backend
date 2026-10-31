//! Single source of truth for which resources sync, how each merges, which
//! columns are derived (never taken from a payload) and the order changes are
//! applied in. Shared by the device (SQLite) and cloud (Mongo) sides so both
//! agree by construction. The older `SYNCABLE` list in `service.rs` keeps
//! serving the legacy `/sync/changes` route and is intentionally left alone.

mod resource_spec;
mod sync_resources;
#[cfg(test)]
mod tests;

pub use resource_spec::MergeClass;
pub use resource_spec::Phase;
pub use resource_spec::ResourceSpec;
pub use resource_spec::label_column;
pub use resource_spec::ordered;
pub use resource_spec::secret_fields;
pub use resource_spec::snake_to_camel;
pub use resource_spec::spec;
pub use resource_spec::without_secrets;
pub use sync_resources::SYNC_RESOURCES;
