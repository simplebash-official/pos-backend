use std::env;

/// Database engine type selected via environment configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    Mongo,
    Sqlite,
}

/// Whether one deployment serves a single shop or many isolated tenants.
/// `Multi` requires MongoDB and makes every database handle tenant-scoped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantMode {
    Single,
    Multi,
}

/// Resolved application configuration. Built once in `main.rs` via
/// `Config::from_env()` and shared through `AppState` — nothing downstream
/// reads environment variables directly.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_type: DatabaseType,
    pub database_url: String,
    pub mongodb_uri: String,
    pub mongodb_db_name: String,
    pub jwt_secret: String,
    pub port: u16,
    /// Address the HTTP listener binds to. Defaults to `0.0.0.0` (all
    /// interfaces) so a container or reverse proxy can route external
    /// traffic in. The Tauri desktop bundle sets this to `127.0.0.1` to
    /// keep the API on loopback only.
    pub bind_addr: String,
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
    /// Whether to automatically seed the database on startup if enabled.
    /// Safe and idempotent (never overwrites existing data).
    pub auto_seed: bool,
    pub tenant_mode: TenantMode,
    /// Browser origins allowed by CORS when `TENANT_MODE=multi`
    /// (`CORS_ALLOWED_ORIGINS`, comma separated). Single-shop deployments
    /// keep the permissive default, since they sit behind their own proxy.
    pub cors_allowed_origins: Vec<String>,
    /// JWKS endpoint of the identity service. When set, EdDSA platform tokens
    /// are accepted alongside local HS256 tokens (see `middleware::platform_jwt`).
    pub identity_jwks_url: Option<String>,
    /// Required `iss` of platform tokens; unset skips the issuer check.
    pub identity_issuer: Option<String>,
    /// Shared secret the identity service presents (`X-Provision-Secret`) when it
    /// hands a newly registered shop to `POST /api/internal/provision`. Unset
    /// switches that endpoint off. Multi-tenant deployments only.
    pub provision_secret: Option<String>,
    /// Environment (e.g. "development" or "production"), parsed from APP_ENV or ENVIRONMENT.
    pub app_env: String,
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
        let db_type_var = env::var("DATABASE_TYPE").ok();
        let db_url_var = env::var("DATABASE_URL").ok();
        let mongo_uri_var = env::var("MONGODB_URI").ok();

        let database_type = match db_type_var.as_deref() {
            Some("sqlite") => DatabaseType::Sqlite,
            Some("mongodb") | Some("mongo") => DatabaseType::Mongo,
            _ => {
                if let Some(ref url) = db_url_var
                    && (url.starts_with("sqlite:") || url.ends_with(".db"))
                {
                    DatabaseType::Sqlite
                } else if mongo_uri_var.is_some() {
                    DatabaseType::Mongo
                } else {
                    // Default to SQLite if neither is explicitly requested
                    DatabaseType::Sqlite
                }
            }
        };

        let (database_url, mongodb_uri, mongodb_db_name) = match database_type {
            DatabaseType::Sqlite => {
                let url = db_url_var.unwrap_or_else(|| "sqlite://data/pos.db?mode=rwc".to_string());
                let mongo_uri = mongo_uri_var.unwrap_or_default();
                let mongo_db = env::var("MONGODB_DB_NAME").unwrap_or_default();
                (url, mongo_uri, mongo_db)
            }
            DatabaseType::Mongo => {
                let mongo_uri = required("MONGODB_URI")?;
                let mongo_db = required("MONGODB_DB_NAME")?;
                let url = db_url_var.unwrap_or_default();
                (url, mongo_uri, mongo_db)
            }
        };

        let jwt_secret = required("JWT_SECRET")?;
        // The .env templates ship a `replace_with_…` placeholder; booting with it
        // (or any short value) would make every token forgeable by anyone who has
        // read the public repo.
        if jwt_secret.starts_with("replace_with") || jwt_secret.len() < 32 {
            return Err(ConfigError::Invalid("JWT_SECRET"));
        }

        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|_| ConfigError::Invalid("PORT"))?;

        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());

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
        let auto_seed = env::var("AUTO_SEED")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes"))
            .unwrap_or(false);

        let tenant_mode = match env::var("TENANT_MODE")
            .unwrap_or_else(|_| "single".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "single" | "" => TenantMode::Single,
            "multi" => TenantMode::Multi,
            _ => return Err(ConfigError::Invalid("TENANT_MODE")),
        };
        // Tenant isolation is implemented on MongoDB only; the SQLite desktop
        // database is one shop by construction.
        if tenant_mode == TenantMode::Multi && database_type != DatabaseType::Mongo {
            return Err(ConfigError::Invalid("TENANT_MODE"));
        }

        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|o| o.trim().trim_end_matches('/').to_string())
            .filter(|o| !o.is_empty())
            .collect();

        let identity_jwks_url = env::var("IDENTITY_JWKS_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if let Some(url) = &identity_jwks_url
            && !(url.starts_with("https://") || url.starts_with("http://"))
        {
            return Err(ConfigError::Invalid("IDENTITY_JWKS_URL"));
        }
        let identity_issuer = env::var("IDENTITY_ISSUER")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let provision_secret = env::var("PROVISION_SECRET")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        // Anything short is guessable over the network; refuse to boot with it.
        if provision_secret.as_deref().is_some_and(|v| v.len() < 16) {
            return Err(ConfigError::Invalid("PROVISION_SECRET"));
        }

        let app_env = env::var("APP_ENV")
            .or_else(|_| env::var("ENVIRONMENT"))
            .unwrap_or_else(|_| "development".to_string())
            .trim()
            .to_lowercase();

        Ok(Self {
            database_type,
            database_url,
            mongodb_uri,
            mongodb_db_name,
            jwt_secret,
            port,
            bind_addr,
            jwt_expiry_hours,
            document_server_url,
            document_server_api_key,
            generated_documents_dir,
            return_window_days,
            auto_seed,
            tenant_mode,
            cors_allowed_origins,
            identity_jwks_url,
            identity_issuer,
            provision_secret,
            app_env,
        })
    }
}

/// Reads a required env var, turning "unset" into a named `ConfigError`
/// instead of a generic `VarError` so the failure message says which
/// variable is missing.
fn required(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::Missing(key))
}
