//! Warranties: one record per covered unit (a sold serial or invoice line), its activity log,
//! the counter lookup, the manager-approved void and extend, and warranty claims (RMA) with
//! their supplier trips. Claims share the warranty's activity log, so they live here.

pub mod migration;
pub mod model;
mod repository;
pub mod routes;
pub(crate) mod service;
