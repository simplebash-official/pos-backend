use super::current_tenant::CURRENT_TENANT;
use super::tenant::Tenant;

/// The ambient tenant of the running request, if one was set.
pub fn current_tenant() -> Option<Tenant> {
    CURRENT_TENANT.try_with(Clone::clone).ok()
}
