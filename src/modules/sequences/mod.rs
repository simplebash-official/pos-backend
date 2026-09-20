mod model;
mod repository;
pub mod routes;
pub mod service;

use crate::{clients::tenant_db::TenantDatabase, core::tenancy::Tenant};

/// `_id` for a counter document. Counters are addressed by a hand-set `_id`
/// (a SKU prefix, a sequence name...), which is unique per collection - so in a
/// multi-tenant database two tenants would collide on the same `_id`. A scoped
/// handle therefore stores `<tenant>:<id>`; the single-shop (`Global`) handle
/// keeps the bare `id`, so existing data and desktop behaviour are unchanged.
pub(crate) fn counter_id(db: &TenantDatabase, id: &str) -> String {
    match db.effective_tenant() {
        Tenant::Global => id.to_string(),
        Tenant::Id(tenant) => format!("{tenant}:{id}"),
        Tenant::Deny => format!("{}:{id}", crate::core::tenancy::DENY_TENANT),
    }
}

