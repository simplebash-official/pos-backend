// Read-only usage figures for one shop, for the operator console (`cloud/admin-backend`): how
// much of the POS a shop actually uses. Counts and sales totals only - never customer names,
// line items or any other record content. Every query runs inside the shop's own tenant scope,
// so the tenant filter is applied by `TenantDatabase` like for any request of that shop; MongoDB
// only (the directory this belongs to exists only in the multi-tenant cloud).

mod day_usage;
mod max_batch;
mod tenant_usage;
mod usage_for;
mod usage_for_many;

pub use day_usage::DayUsage;
pub use max_batch::MAX_BATCH;
pub use tenant_usage::TenantUsage;
pub use usage_for::usage_for;
pub use usage_for_many::usage_for_many;
