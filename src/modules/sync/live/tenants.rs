use super::live_event::LiveEvent;

use std::collections::HashMap;

use std::sync::{Mutex, OnceLock};

use tokio::sync::broadcast;

// ----------------------------------------------------------------------------
// Cloud: per-tenant fan-out
// ----------------------------------------------------------------------------
pub(super) fn tenants() -> &'static Mutex<HashMap<String, broadcast::Sender<LiveEvent>>> {
    static TENANTS: OnceLock<Mutex<HashMap<String, broadcast::Sender<LiveEvent>>>> =
        OnceLock::new();
    TENANTS.get_or_init(|| Mutex::new(HashMap::new()))
}
