use super::resource_spec::ResourceSpec;
use super::sync_resources::SYNC_RESOURCES;

/// Resources in apply order (parents first).
pub fn ordered() -> impl Iterator<Item = &'static ResourceSpec> {
    SYNC_RESOURCES.iter()
}
