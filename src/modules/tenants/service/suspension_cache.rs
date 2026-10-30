use super::suspension_info::SuspensionInfo;

pub(super) type SuspensionCache =
    std::collections::HashMap<String, (Option<SuspensionInfo>, std::time::Instant)>;

static SUSPENDED: std::sync::LazyLock<std::sync::Mutex<SuspensionCache>> =
    std::sync::LazyLock::new(Default::default);

pub(super) fn suspension_cache() -> std::sync::MutexGuard<'static, SuspensionCache> {
    SUSPENDED.lock().expect("suspension cache mutex poisoned")
}
