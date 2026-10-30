/// Origin of a write made outside any device or browser session (background
/// work, internal provisioning). No device ever has this id, so every device
/// receives the change.
pub const CLOUD_ORIGIN: &str = "cloud";
