/// Why startup configuration failed to load. `main.rs` logs this and exits
/// rather than letting the process start half-configured.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required env var {0}")]
    Missing(&'static str),
    #[error("invalid value for env var {0}")]
    Invalid(&'static str),
}
