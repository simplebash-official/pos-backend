// The income ledger: money received that is not a sale. Cash put into the drawer is an add-cash
// movement and is read from there; this module stores what was received into the bank, a card or
// online.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
