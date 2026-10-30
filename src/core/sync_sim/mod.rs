// Deterministic in-memory model of sync v2: replicas that apply changes through
// the real merge engine (`core::sync_merge`), a seeded scenario generator, and a
// hub-and-spoke harness (devices <-> one "cloud" replica). Used by the unit
// tests here and by the convergence harness that drives real backends.
//
// Everything is pure and seeded: the same seed always produces the same run.
// Payload field names mirror the camelCase DTOs the real resources use.

mod apply_result;
mod get_i64;
mod get_str;
mod harness;
mod iso;
mod op;
mod rng;
mod scenario;
mod sim_device;
mod sim_replica;
mod sim_row;
#[cfg(test)]
mod tests;

pub use apply_result::ApplyResult;
pub use harness::Harness;
pub use op::Op;
pub use rng::Rng;
pub use scenario::Scenario;
pub use sim_device::SimDevice;
pub use sim_replica::SimReplica;
pub use sim_row::SimRow;
