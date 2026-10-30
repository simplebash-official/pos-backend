// Persistence for the platform `tenants` directory. Mongo only: the directory
// exists solely for multi-tenant deployments. No `AppError`s here beyond the
// engine check - not-found/duplicate decisions belong to `service`.

mod complete_tenant_setup;
mod directory;
mod find_tenant_by_key;
mod find_tenant_by_shop_code;
mod insert_tenant;
mod set_tenant_suspended;
mod update_tenant_name;

pub(crate) use complete_tenant_setup::complete_tenant_setup;
pub(crate) use find_tenant_by_key::find_tenant_by_key;
pub(crate) use find_tenant_by_shop_code::find_tenant_by_shop_code;
pub(crate) use insert_tenant::insert_tenant;
pub(crate) use set_tenant_suspended::set_tenant_suspended;
pub(crate) use update_tenant_name::update_tenant_name;
