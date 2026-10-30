// HTTP surface of the shop's custom design packages. Cloud (multi-tenant)
// only: a desktop's own backend has no packages to offer, so the routes answer
// 404 there. Authenticated by any signed-in shop token — web users and the
// desktop sync agent's device token alike — and scoped to that token's shop by
// `X-Tenant-Key` on the document-server call.

mod design_package;
mod design_packages_response;
mod list_packages;
mod package_archive;
mod require_cloud;
mod router;

pub use design_package::DesignPackage;
pub use design_packages_response::DesignPackagesResponse;
pub use router::router;
