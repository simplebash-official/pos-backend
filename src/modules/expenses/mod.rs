// The expense ledger: every business cost, whichever account it moved through. A cost paid from
// the drawer is a cash movement and is read from there; this module stores the rest (bank, card,
// online, or a bill still to pay) and the payments made against it.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
