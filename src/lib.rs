// Library crate re-exporting every module so `main.rs`, `src/bin/*.rs`
// (separate binary crates), and `tests/*.rs` (separate integration-test
// crates) can all share the same code via `simplebash_pos_backend::...` — none
// of them can reach `main.rs`'s `crate::` paths directly.
pub mod app;
pub mod clients;
pub mod core;
pub mod domain;
pub mod modules;
pub mod seeds;
pub mod workers;
