use super::sim_replica::SimReplica;

/// One simulated device plus its link state to the hub.
#[derive(Debug, Clone)]
pub struct SimDevice {
    pub replica: SimReplica,
    pub online: bool,
    /// How much of the hub log this device has consumed.
    pub cursor: usize,
    pub(super) counter: u64,
}
