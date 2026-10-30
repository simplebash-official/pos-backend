use super::resource_spec::ResourceSpec;
use super::sync_resources::SYNC_RESOURCES;

/// Looks a resource up by wire name or table name.
#[must_use]
pub fn spec(name: &str) -> Option<&'static ResourceSpec> {
    SYNC_RESOURCES
        .iter()
        .find(|s| s.name == name || s.table == name)
}
