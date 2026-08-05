use std::env;

/// Resolved application configuration. Built once in `main.rs` via
/// `Config::from_env()` and shared through `AppState` — nothing downstream
/// reads environment variables directly.
#[derive(Debug, Clone)]
pub struct Config {
    pub mongodb_uri: String,
    pub mongodb_db_name: String,
    pub jwt_secret: String,
    pub port: u16,
    pub jwt_expiry_hours: i64,
}

/// Why startup configuration failed to load. `main.rs` logs this and exits
/// rather than letting the process start half-configured.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required env var {0}")]
    Missing(&'static str),
    #[error("invalid value for env var {0}")]
    Invalid(&'static str),
}

impl Config {
    /// Parses and validates all required configuration once, at startup.
    /// Fails fast so a misconfigured deployment never reaches request-serving code.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mongodb_uri = required("MONGODB_URI")?;
        let mongodb_db_name = required("MONGODB_DB_NAME")?;
        let jwt_secret = required("JWT_SECRET")?;

        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|_| ConfigError::Invalid("PORT"))?;

        // How long a login's JWT stays valid (there is no refresh-token or
        // revocation flow; a caller just re-authenticates via login once
        // this expires) — required, not defaulted, so an operator makes an
        // explicit choice rather than silently inheriting a hardcoded value.
        let jwt_expiry_hours = required("JWT_EXPIRY_HOURS")?
            .parse::<i64>()
            .map_err(|_| ConfigError::Invalid("JWT_EXPIRY_HOURS"))?;

        Ok(Self {
            mongodb_uri,
            mongodb_db_name,
            jwt_secret,
            port,
            jwt_expiry_hours,
        })
    }
}

/// Reads a required env var, turning "unset" into a named `ConfigError`
/// instead of a generic `VarError` so the failure message says which
/// variable is missing.
fn required(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::Missing(key))
}
