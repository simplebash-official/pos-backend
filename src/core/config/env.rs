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
    /// Base URL of the sibling document-server (see `clients::document_server`).
    /// No default — a misconfigured deployment should fail startup rather
    /// than silently produce broken "print invoice" requests.
    pub document_server_url: String,
    /// Shared secret sent as `X-Internal-Api-Key` on every document-server
    /// call. Must match that service's own `INTERNAL_API_KEY`.
    pub document_server_api_key: String,
    /// Filesystem directory (relative to the working directory the binary
    /// runs from, or absolute) generated invoice/receipt PDFs are saved
    /// under — see `modules::documents::service`. Defaulted, unlike the two
    /// fields above, since a sensible relative default is safe either way.
    pub generated_documents_dir: String,
    /// How many days after an invoice's `created_at` a credit note may
    /// still be created against it without a manager override. Defaulted
    /// (unlike `mongodb_uri`/`jwt_secret`) since it's a business-tunable
    /// policy value, not a startup-integrity concern.
    pub return_window_days: i64,
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

        let document_server_url = required("DOCUMENT_SERVER_URL")?;
        let document_server_api_key = required("DOCUMENT_SERVER_API_KEY")?;
        let generated_documents_dir = env::var("GENERATED_DOCUMENTS_DIR")
            .unwrap_or_else(|_| "generated_documents".to_string());
        let return_window_days = env::var("RETURN_WINDOW_DAYS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(30);

        Ok(Self {
            mongodb_uri,
            mongodb_db_name,
            jwt_secret,
            port,
            jwt_expiry_hours,
            document_server_url,
            document_server_api_key,
            generated_documents_dir,
            return_window_days,
        })
    }
}

/// Reads a required env var, turning "unset" into a named `ConfigError`
/// instead of a generic `VarError` so the failure message says which
/// variable is missing.
fn required(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::Missing(key))
}
