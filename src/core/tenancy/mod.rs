// Tenant isolation core: the `Tenant` scope a database handle carries and the
// pure rewrites (filters, aggregation pipelines, inserted documents) that make
// every MongoDB read and write stay inside one tenant. Nothing here does I/O;
// `clients::tenant_db` applies these rewrites to the real driver calls so a
// repository cannot forget the tenant filter.

mod current_tenant;
mod current_tenant_fn;
mod current_tenant_id;
mod deny_tenant;
mod scope_filter;
mod scope_pipeline;
mod stamp_document;
mod tenant;
mod tenant_field;
#[cfg(test)]
mod tests;
mod with_tenant;

pub use current_tenant_fn::current_tenant;
pub use current_tenant_id::current_tenant_id;
pub use deny_tenant::DENY_TENANT;
pub use scope_filter::scope_filter;
pub use scope_pipeline::scope_pipeline;
pub use stamp_document::stamp_document;
pub use tenant::Tenant;
pub use tenant_field::TENANT_FIELD;
pub use with_tenant::with_tenant;
