// Platform tenant directory: maps a human-friendly shop code ("acme-repairs")
// to the tenant id a login is confined to. The `tenants` collection belongs to
// no tenant, so it is read and written through the unfiltered platform handle
// (`TenantDatabase::platform_collection`). There are deliberately no HTTP
// routes yet: tenants are created by `cargo run --bin create_tenant` (and, later,
// by the cloud identity service); login only resolves codes through `service`.
mod model;
mod repository;
pub mod service;
