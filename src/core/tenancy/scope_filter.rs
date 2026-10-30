use super::tenant::Tenant;
use super::tenant_field::TENANT_FIELD;

use mongodb::bson::{Document, doc};

/// Confines a query filter to the tenant. The original filter is nested under
/// `$and` so nothing inside it (an `$or`, or an explicit `tenant_id` key from a
/// caller) can widen the tenant condition.
#[must_use]
pub fn scope_filter(tenant: &Tenant, filter: Document) -> Document {
    match tenant.value() {
        None => filter,
        Some(t) if filter.is_empty() => doc! { TENANT_FIELD: t },
        Some(t) => doc! { "$and": [ { TENANT_FIELD: t }, filter ] },
    }
}
