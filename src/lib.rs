//! The SimpleBash point-of-sale backend as a library, so `main.rs`, the binaries in `src/bin` and
//! the integration tests in `tests` share one set of modules through `simplebash_pos_backend::...`.

pub mod app;
pub mod clients;
pub mod core;
pub mod domain;
pub mod modules;
pub mod seeds;
pub mod workers;
