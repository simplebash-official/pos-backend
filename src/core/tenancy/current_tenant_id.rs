use super::current_tenant_fn::current_tenant;
use super::tenant::Tenant;

/// Id of the ambient tenant, if the running request is confined to one.
#[must_use]
pub fn current_tenant_id() -> Option<String> {
    match current_tenant() {
        Some(Tenant::Id(id)) => Some(id.to_string()),
        _ => None,
    }
}
