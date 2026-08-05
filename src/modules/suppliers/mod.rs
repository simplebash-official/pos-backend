// Supplier CRUD: contact/business info plus the free-text category tags a
// supplier deals in. `repository`/`service` are private/`pub(crate)`-gated
// so nothing outside this module tree can reach past `routes` into
// persistence (see `modules::inventory` for the reference pattern this
// mirrors). `model` stays `pub` like inventory's, since integration tests
// need `SupplierDocument` to seed fixture data directly.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
