use super::live_event::LiveEvent;
use super::tenants::tenants;

use tokio::sync::broadcast;

/// Notifications buffered per tenant before a slow subscriber is told to
/// resync (it then pulls, which loses nothing).
const CHANNEL_CAPACITY: usize = 1024;

/// Receives every change of `tenant` announced from now on.
#[must_use]
pub fn subscribe(tenant: &str) -> broadcast::Receiver<LiveEvent> {
    let mut map = tenants()
        .lock()
        .expect("live sync channel registry mutex poisoned");
    map.entry(tenant.to_string())
        .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
        .subscribe()
}
