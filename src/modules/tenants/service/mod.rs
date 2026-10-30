//! Business rules for the tenant directory: shop-code format, creation, and
//! resolving a code to the `Tenant` scope a login runs in.

mod tenants_service;
#[cfg(test)]
mod tests;

pub use tenants_service::create_tenant;
pub use tenants_service::create_tenant_with_key;
pub use tenants_service::document_selections;

pub use tenants_service::ProvisionOutcome;
pub use tenants_service::ProvisionOwner;
pub(crate) use tenants_service::lookup_shop_details;
pub use tenants_service::normalize_shop_code;
pub use tenants_service::provision_shop;
pub use tenants_service::reset_document_selections;
pub use tenants_service::resolve_shop_code;

pub use tenants_service::set_suspended;
pub(crate) use tenants_service::suspended_error;

pub use tenants_service::TenantInfo;
pub(crate) use tenants_service::suspension_of;
