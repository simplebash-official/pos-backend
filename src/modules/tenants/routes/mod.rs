// HTTP surface of the tenant directory: the one server-to-server endpoint the
// identity service calls when a shop registers. Not a browser API — it is
// authenticated by a shared secret (never a bearer token) and nginx refuses to
// forward `/api/internal/` from the internet, so only containers on the compose
// network can reach it.

mod authorize;
mod batch_usage;
mod batch_usage_request;
mod batch_usage_response;
mod document_selection;
mod document_selections_response;
mod provision_owner_request;
mod provision_shop;
mod provision_shop_request;
mod provision_shop_response;
mod reset_selections_request;
mod reset_selections_response;
mod reset_shop_document_selections;
mod router;
mod secrets_match;
mod set_status;
mod set_status_request;
mod shop_activities;
mod shop_activities_query;
mod shop_activities_response;
mod shop_activity_item;
mod shop_document_selections;
mod shop_login_item;
mod shop_logins;
mod shop_logins_query;
mod shop_logins_response;
mod shop_usage;
mod shop_user_response;
mod shop_users;
mod status_response;
#[cfg(test)]
mod tests;
mod to_selections;

pub use batch_usage_request::BatchUsageRequest;
pub use batch_usage_response::BatchUsageResponse;
pub use document_selection::DocumentSelection;
pub use document_selections_response::DocumentSelectionsResponse;
pub use provision_owner_request::ProvisionOwnerRequest;
pub use provision_shop_request::ProvisionShopRequest;
pub use provision_shop_response::ProvisionShopResponse;
pub use reset_selections_request::ResetSelectionsRequest;
pub use reset_selections_response::ResetSelectionsResponse;
pub use router::router;
pub use set_status_request::SetStatusRequest;
pub use shop_activities_query::ShopActivitiesQuery;
pub use shop_activities_response::ShopActivitiesResponse;
pub use shop_activity_item::ShopActivityItem;
pub use shop_login_item::ShopLoginItem;
pub use shop_logins_query::ShopLoginsQuery;
pub use shop_logins_response::ShopLoginsResponse;
pub use shop_user_response::ShopUserResponse;
pub use status_response::StatusResponse;
