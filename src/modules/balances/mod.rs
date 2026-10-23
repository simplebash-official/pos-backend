// What the shop owes suppliers and what staff owe on advances. Both are worked out from purchases,
// drawer payouts and ledger payments elsewhere; this module stores only where a balance starts and
// the part of an advance taken back from pay.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
