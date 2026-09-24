// Platform tenant directory: maps a human-friendly shop code ("acme-repairs")
// to the tenant id a login is confined to. The `tenants` collection belongs to
// no tenant, so it is read and written through the unfiltered platform handle
// (`TenantDatabase::platform_collection`). The only HTTP route is the
// server-to-server `POST /api/internal/provision` the identity service calls when
// a shop registers (`routes`); tenants can also be created by hand with
// `cargo run --bin create_tenant`. Login only resolves codes through `service`.
mod model;
mod repository;
pub mod routes;
pub mod service;
