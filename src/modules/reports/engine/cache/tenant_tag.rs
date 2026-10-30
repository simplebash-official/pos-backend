/// Tag attached to every entry of a tenant, used to scope invalidation.
#[must_use]
pub fn tenant_tag(tenant: &str) -> String {
    format!("tenant:{tenant}")
}
