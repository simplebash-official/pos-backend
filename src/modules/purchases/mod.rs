// Supplier purchase / stock-intake history: recording a stock receipt
// (which also bumps the referenced product's stock and writes a
// `PurchaseReceipt` movement in `inventory`) and listing purchase history
// by supplier or product. `model`/`repository` are private so nothing
// outside this module tree can reach past `routes`/`service` into
// persistence (see `modules::inventory` for the reference pattern this
// mirrors).
mod model;
mod repository;
pub mod routes;
pub mod service;
