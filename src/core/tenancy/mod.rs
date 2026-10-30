//! Tenant isolation core: the `Tenant` scope a database handle carries and the
//! pure rewrites (filters, aggregation pipelines, inserted documents) that make
//! every MongoDB read and write stay inside one tenant. Nothing here does I/O;
//! `clients::tenant_db` applies these rewrites to the real driver calls so a
//! repository cannot forget the tenant filter.

mod core_tenancy;
#[cfg(test)]
mod tests;

pub use core_tenancy::DENY_TENANT;
pub use core_tenancy::TENANT_FIELD;
pub use core_tenancy::Tenant;
pub use core_tenancy::current_tenant;
pub use core_tenancy::current_tenant_id;
pub use core_tenancy::scope_filter;
pub use core_tenancy::scope_pipeline;
pub use core_tenancy::stamp_document;
pub use core_tenancy::with_tenant;
