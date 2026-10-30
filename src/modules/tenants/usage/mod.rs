//! Read-only usage figures for one shop, for the operator console (`cloud/admin-backend`): how
//! much of the POS a shop actually uses. Counts and sales totals only - never customer names,
//! line items or any other record content. Every query runs inside the shop's own tenant scope,
//! so the tenant filter is applied by `TenantDatabase` like for any request of that shop; MongoDB
//! only (the directory this belongs to exists only in the multi-tenant cloud).

mod tenants_usage;

pub use tenants_usage::DayUsage;
pub use tenants_usage::MAX_BATCH;
pub use tenants_usage::TenantUsage;
pub use tenants_usage::usage_for;
pub use tenants_usage::usage_for_many;
