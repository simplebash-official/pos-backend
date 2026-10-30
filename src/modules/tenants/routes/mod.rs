//! HTTP surface of the tenant directory: the one server-to-server endpoint the
//! identity service calls when a shop registers. Not a browser API — it is
//! authenticated by a shared secret (never a bearer token) and nginx refuses to
//! forward `/api/internal/` from the internet, so only containers on the compose
//! network can reach it.

mod shop_activities;
mod tenants_routes;
#[cfg(test)]
mod tests;

pub use tenants_routes::BatchUsageRequest;
pub use tenants_routes::BatchUsageResponse;
pub use tenants_routes::DocumentSelection;
pub use tenants_routes::DocumentSelectionsResponse;
pub use tenants_routes::ProvisionOwnerRequest;
pub use tenants_routes::ProvisionShopRequest;
pub use tenants_routes::ProvisionShopResponse;
pub use tenants_routes::ResetSelectionsRequest;
pub use tenants_routes::ResetSelectionsResponse;
pub use tenants_routes::SetStatusRequest;
pub use tenants_routes::ShopActivitiesQuery;
pub use tenants_routes::ShopActivitiesResponse;
pub use tenants_routes::ShopActivityItem;
pub use tenants_routes::ShopLoginItem;
pub use tenants_routes::ShopLoginsQuery;
pub use tenants_routes::ShopLoginsResponse;
pub use tenants_routes::ShopUserResponse;
pub use tenants_routes::StatusResponse;
pub use tenants_routes::router;
