static SERVER_STARTED_AT: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

pub fn server_uptime_seconds() -> u64 {
    SERVER_STARTED_AT.elapsed().as_secs()
}
