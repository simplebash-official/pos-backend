// The many-to-many join between suppliers and inventory products:
// upsert-on-link, unlink, and a bulk-replace-all-links-for-a-supplier
// endpoint. `model`/`repository` are private so nothing outside this module
// tree can reach past `routes`/`service` into persistence (see
// `modules::inventory` for the reference pattern this mirrors).
mod model;
mod repository;
pub mod routes;
pub mod service;
