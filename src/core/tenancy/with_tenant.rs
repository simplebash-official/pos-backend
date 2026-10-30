use super::current_tenant::CURRENT_TENANT;
use super::tenant::Tenant;

use std::future::Future;

/// Runs `fut` with `tenant` as the ambient tenant of every multi-tenant
/// database handle opened inside it (the request middleware wraps each
/// authenticated request this way). The value does not follow a `tokio::spawn`,
/// which is deliberate: work that escapes the request scope fails closed.
pub async fn with_tenant<F: Future>(tenant: Tenant, fut: F) -> F::Output {
    CURRENT_TENANT.scope(tenant, fut).await
}
