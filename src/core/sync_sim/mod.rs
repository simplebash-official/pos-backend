//! Deterministic in-memory model of sync v2: replicas that apply changes through
//! the real merge engine (`core::sync_merge`), a seeded scenario generator, and a
//! hub-and-spoke harness (devices <-> one "cloud" replica). Used by the unit
//! tests here and by the convergence harness that drives real backends.
//!
//! Everything is pure and seeded: the same seed always produces the same run.
//! Payload field names mirror the camelCase DTOs the real resources use.

mod core_sync_sim;
mod harness;
mod sim_replica;
#[cfg(test)]
mod tests;

pub use core_sync_sim::ApplyResult;
pub use core_sync_sim::Op;
pub use core_sync_sim::Rng;
pub use core_sync_sim::Scenario;
pub use core_sync_sim::SimDevice;
pub use core_sync_sim::SimRow;
pub use harness::Harness;
pub use sim_replica::SimReplica;
