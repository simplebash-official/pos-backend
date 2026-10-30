use super::live_event::LiveEvent;
use super::tenants::tenants;

/// Announces a change to the tenant's subscribers (dropping the channel once
/// nobody listens any more).
pub fn publish(tenant: &str, event: LiveEvent) {
    let mut map = tenants()
        .lock()
        .expect("live sync channel registry mutex poisoned");
    if let Some(sender) = map.get(tenant)
        && sender.send(event).is_err()
    {
        map.remove(tenant);
    }
}
