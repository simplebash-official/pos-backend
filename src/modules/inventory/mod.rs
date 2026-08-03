// Reference implementation for a fully built-out feature module: product
// CRUD, stock adjustment/history, and category/subcategory management,
// layered as model (BSON shapes) -> repository (Mongo only) -> service
// (business rules) -> routes (HTTP). `repository`/`service` are private so
// nothing outside this module tree can reach past `routes`.
pub mod model;
mod repository;
pub mod routes;
mod service;
