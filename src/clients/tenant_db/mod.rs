//! Tenant-aware MongoDB handle. `TenantDatabase` is what `Db::Mongo` holds and
//! `ScopedCollection` is what repositories get from `.collection(..)`: every
//! read, write and aggregation is rewritten through `core::tenancy` before it
//! reaches the driver, so a repository cannot forget the tenant filter. With
//! `Tenant::Global` the rewrites are identity, i.e. the raw driver behaviour.
//!
//! Writes to a synced collection (`core::sync_origin::SYNCED_COLLECTIONS`) on a
//! multi-tenant handle are also stamped with the request's origin
//! (`updated_by_device`), so the sync change log always names the real writer.
//!
//! Deliberately NOT here: index creation and other admin operations. Those need
//! the raw handle (`TenantDatabase::unscoped`).

mod clients_tenant_db;

pub use clients_tenant_db::ScopedCollection;
pub use clients_tenant_db::TenantDatabase;
