use std::sync::OnceLock;

use tokio::sync::watch;

// ----------------------------------------------------------------------------
// Desktop: "the outbox may have grown"
// ----------------------------------------------------------------------------
pub(super) fn local_writes() -> &'static watch::Sender<u64> {
    static WRITES: OnceLock<watch::Sender<u64>> = OnceLock::new();
    WRITES.get_or_init(|| watch::channel(0).0)
}
