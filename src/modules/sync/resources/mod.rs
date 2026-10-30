// Single source of truth for which resources sync, how each merges, which
// columns are derived (never taken from a payload) and the order changes are
// applied in. Shared by the device (SQLite) and cloud (Mongo) sides so both
// agree by construction. The older `SYNCABLE` list in `service.rs` keeps
// serving the legacy `/sync/changes` route and is intentionally left alone.

mod label_column;
mod merge_class;
mod ordered;
mod phase;
mod resource_spec;
mod secret_fields;
mod snake_to_camel;
mod spec;
mod sync_resources;
#[cfg(test)]
mod tests;
mod without_secrets;

pub use label_column::label_column;
pub use merge_class::MergeClass;
pub use ordered::ordered;
pub use phase::Phase;
pub use resource_spec::ResourceSpec;
pub use secret_fields::secret_fields;
pub use snake_to_camel::snake_to_camel;
pub use spec::spec;
pub use sync_resources::SYNC_RESOURCES;
pub use without_secrets::without_secrets;
