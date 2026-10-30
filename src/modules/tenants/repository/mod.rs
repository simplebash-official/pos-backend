//! Persistence for the platform `tenants` directory. Mongo only: the directory
//! exists solely for multi-tenant deployments. No `AppError`s here beyond the
//! engine check - not-found/duplicate decisions belong to `service`.

mod tenants_repository;

pub(crate) use tenants_repository::complete_tenant_setup;
pub(crate) use tenants_repository::find_tenant_by_key;
pub(crate) use tenants_repository::find_tenant_by_shop_code;
pub(crate) use tenants_repository::insert_tenant;
pub(crate) use tenants_repository::set_tenant_suspended;
pub(crate) use tenants_repository::update_tenant_name;
