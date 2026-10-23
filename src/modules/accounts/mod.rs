// The shop's money accounts: the drawer, the bank, cards and online. Balances are worked out from
// activity elsewhere (sales payments, income, expenses, cash drops); this module stores only where
// each account started.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
