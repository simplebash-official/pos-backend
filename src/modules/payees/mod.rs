// The shared payee directory a cash movement names. Suppliers and staff stay in their
// own modules and are listed here by reference; only vendors (landlords, utility
// companies) and free-typed payees are stored, in `payees`.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
