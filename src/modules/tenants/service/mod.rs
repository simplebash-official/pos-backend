// Business rules for the tenant directory: shop-code format, creation, and
// resolving a code to the `Tenant` scope a login runs in.

mod create_tenant;
mod create_tenant_with_key;
mod document_selections;
mod in_shop;
mod lookup_shop_code;
mod lookup_shop_details;
mod normalize_shop_code;
mod provision_outcome;
mod provision_owner;
mod provision_shop;
mod reset_document_selections;
mod resolve_shop_code;
mod retained;
mod set_suspended;
mod suspended_error;
mod suspension_cache;
mod suspension_info;
mod suspension_of;
mod tenant_info;
#[cfg(test)]
mod tests;
mod validate_tenant_key;

pub use create_tenant::create_tenant;
pub use create_tenant_with_key::create_tenant_with_key;
pub use document_selections::document_selections;

pub(crate) use lookup_shop_details::lookup_shop_details;
pub use normalize_shop_code::normalize_shop_code;
pub use provision_outcome::ProvisionOutcome;
pub use provision_owner::ProvisionOwner;
pub use provision_shop::provision_shop;
pub use reset_document_selections::reset_document_selections;
pub use resolve_shop_code::resolve_shop_code;

pub use set_suspended::set_suspended;
pub(crate) use suspended_error::suspended_error;

pub(crate) use suspension_of::suspension_of;
pub use tenant_info::TenantInfo;
