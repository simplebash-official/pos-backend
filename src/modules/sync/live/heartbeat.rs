use std::time::Duration;

/// Comment line sent while idle, so proxies keep the stream open and clients
/// notice a dead connection.
pub const HEARTBEAT: Duration = Duration::from_secs(15);
