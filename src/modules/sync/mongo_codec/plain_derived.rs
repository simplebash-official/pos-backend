use crate::modules::sync::resources::ResourceSpec;

/// The top-level derived columns of a resource. The spec also lists nested
/// (`items[].returned_quantity`) and lifecycle (`status`) entries; those are
/// merged by other rules and must not be treated as plain derived columns.
pub(crate) fn plain_derived(spec: &ResourceSpec) -> Vec<&'static str> {
    spec.derived
        .iter()
        .copied()
        .filter(|c| !c.contains('[') && *c != "status")
        .collect()
}
