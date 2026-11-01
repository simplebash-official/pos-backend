//! Document-number sequences reserved by devices that create documents offline.

mod counter_id;
pub(crate) mod model;
mod repository;
pub mod routes;
pub mod service;

pub(crate) use counter_id::counter_id;
