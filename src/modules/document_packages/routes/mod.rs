//! HTTP surface of the shop's custom design packages. Cloud (multi-tenant)
//! only: a desktop's own backend has no packages to offer, so the routes answer
//! 404 there. Authenticated by any signed-in shop token — web users and the
//! desktop sync agent's device token alike — and scoped to that token's shop by
//! `X-Tenant-Key` on the document-server call.

mod document_packages_routes;

pub use document_packages_routes::DesignPackage;
pub use document_packages_routes::DesignPackagesResponse;
pub use document_packages_routes::router;
