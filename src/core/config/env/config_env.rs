use crate::core::logging::SqlLogging;
use std::str::FromStr;

/// Whether this process is a development or a production deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnvironment {
    Development,
    Production,
}

impl AppEnvironment {
    /// The name this environment is configured and reported under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AppEnvironment::Development => "development",
            AppEnvironment::Production => "production",
        }
    }
}

/// Resolved application configuration. Built once at startup by `Config::from_env()` and
/// shared through `AppState` — nothing downstream reads environment variables directly.
/// Every variable is required: a missing or invalid one stops the process at startup.
#[derive(Clone)]
pub struct Config {
    pub database: Database,
    pub jwt_secret: String,
    pub port: u16,
    /// Address the HTTP listener binds to. `0.0.0.0` lets a container or reverse proxy route
    /// external traffic in; the desktop bundle uses `127.0.0.1` to keep the API on loopback.
    pub bind_addr: String,
    pub jwt_expiry_hours: i64,
    /// Base URL of the sibling document-server (see `clients::document_server`).
    pub document_server_url: String,
    /// Shared secret sent as `X-Internal-Api-Key` on every document-server call. Must match
    /// that service's own `INTERNAL_API_KEY`.
    pub document_server_api_key: String,
    /// Directory generated invoice/receipt PDFs are saved under, relative to the working
    /// directory or absolute — see `modules::documents::service`.
    pub generated_documents_dir: String,
    /// How many days after an invoice's `created_at` a credit note may still be created
    /// against it without a manager override.
    pub return_window_days: i64,
    /// Whether to seed the database on startup. Safe and idempotent (never overwrites data).
    pub auto_seed: bool,
    pub tenant_mode: TenantMode,
    /// Browser origins allowed by CORS, in every tenant mode; empty means no browser origin.
    /// Never `*`: a desktop backend on 127.0.0.1 is reachable from any page the owner opens.
    pub cors_allowed_origins: Vec<String>,
    pub identity_auth: IdentityAuth,
    pub provisioning: Provisioning,
    pub app_env: AppEnvironment,
    pub rate_limits: RateLimits,
    pub logging: LoggingConfig,
    /// The id this installation reports in its installation record and sync captures.
    pub installation_id: String,
    /// The platform this installation reports, e.g. `macos` or `cloud`.
    pub platform: String,
    pub seed_admin: SeedAdminConfig,
}

/// Prints every field except the secrets, which show as `<redacted>`, so a stray
/// `{:?}` of the configuration never reaches a log.
impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const REDACTED: &str = "<redacted>";
        formatter
            .debug_struct("Config")
            .field("database", &self.database)
            .field("jwt_secret", &REDACTED)
            .field("port", &self.port)
            .field("bind_addr", &self.bind_addr)
            .field("jwt_expiry_hours", &self.jwt_expiry_hours)
            .field("document_server_url", &self.document_server_url)
            .field("document_server_api_key", &REDACTED)
            .field("generated_documents_dir", &self.generated_documents_dir)
            .field("return_window_days", &self.return_window_days)
            .field("auto_seed", &self.auto_seed)
            .field("tenant_mode", &self.tenant_mode)
            .field("cors_allowed_origins", &self.cors_allowed_origins)
            .field("identity_auth", &self.identity_auth)
            .field("provisioning", &self.provisioning)
            .field("app_env", &self.app_env)
            .field("rate_limits", &self.rate_limits)
            .field("logging", &self.logging)
            .field("installation_id", &self.installation_id)
            .field("platform", &self.platform)
            .field("seed_admin", &self.seed_admin)
            .finish()
    }
}

impl Config {
    /// The MongoDB URI and database name, for tools that only work against MongoDB.
    pub fn mongo_settings(&self) -> Result<(&str, &str), ConfigError> {
        match &self.database {
            Database::Mongo { uri, name } => Ok((uri, name)),
            Database::Sqlite { .. } => Err(ConfigError::Invalid("DATABASE_TYPE")),
        }
    }

    /// Parses and validates the process environment. Fails fast so a misconfigured deployment
    /// never reaches request-serving code.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_variables(&Variables::new(&|name| std::env::var(name).ok()))
    }

    /// Parses and validates `variables`; see `from_env`.
    pub fn from_variables(variables: &Variables<'_>) -> Result<Self, ConfigError> {
        let tenant_mode = match variables.one_of("TENANT_MODE", &["single", "multi"])? {
            0 => TenantMode::Single,
            _ => TenantMode::Multi,
        };

        let database = match variables.one_of("DATABASE_TYPE", &["sqlite", "mongodb"])? {
            0 => Database::Sqlite {
                url: variables.required("DATABASE_URL")?,
            },
            _ => Database::Mongo {
                uri: variables.required("MONGODB_URI")?,
                name: variables.required("MONGODB_DB_NAME")?,
            },
        };
        // Tenant isolation is implemented on MongoDB only; the SQLite desktop database is one
        // shop by construction.
        if tenant_mode == TenantMode::Multi && !matches!(database, Database::Mongo { .. }) {
            return Err(ConfigError::Invalid("TENANT_MODE"));
        }

        let jwt_secret = variables.required("JWT_SECRET")?;
        // The .env templates ship a `replace_with_…` placeholder; booting with it (or any short
        // value) would make every token forgeable by anyone who has read the public repo.
        if jwt_secret.starts_with("replace_with") || jwt_secret.len() < 32 {
            return Err(ConfigError::Invalid("JWT_SECRET"));
        }

        let document_server_api_key = variables.required("DOCUMENT_SERVER_API_KEY")?;
        // The document server refuses to boot with a copied-from-the-docs key; fail here too,
        // with the variable name that actually needs changing.
        let key = document_server_api_key.to_ascii_lowercase();
        if key.starts_with("replace_with") || key.starts_with("change-me") {
            return Err(ConfigError::Invalid("DOCUMENT_SERVER_API_KEY"));
        }

        let cors_allowed_origins: Vec<String> = variables
            .required_allowing_empty("CORS_ALLOWED_ORIGINS")?
            .split(',')
            .map(|origin| origin.trim().trim_end_matches('/').to_string())
            .filter(|origin| !origin.is_empty())
            .collect();
        for origin in &cors_allowed_origins {
            if origin.parse::<axum::http::HeaderValue>().is_err() {
                return Err(ConfigError::Invalid("CORS_ALLOWED_ORIGINS"));
            }
        }

        let identity_auth = match variables.one_of("IDENTITY_AUTH", &["disabled", "enabled"])? {
            0 => IdentityAuth::Disabled,
            _ => {
                let jwks_url = variables.required("IDENTITY_JWKS_URL")?;
                if !(jwks_url.starts_with("https://") || jwks_url.starts_with("http://")) {
                    return Err(ConfigError::Invalid("IDENTITY_JWKS_URL"));
                }
                IdentityAuth::Enabled {
                    jwks_url,
                    // Without an issuer pin, any token signed by a key in that JWKS is accepted
                    // regardless of who minted it for what.
                    issuer: variables.required("IDENTITY_ISSUER")?,
                    // In single-shop mode a platform token is only accepted for this shop;
                    // otherwise any account on the identity service would become its Admin.
                    tenant_id: match tenant_mode {
                        TenantMode::Single => Some(variables.required("IDENTITY_TENANT_ID")?),
                        TenantMode::Multi => None,
                    },
                }
            }
        };

        let provisioning = match variables.one_of("PROVISIONING", &["disabled", "enabled"])? {
            0 => Provisioning::Disabled,
            _ => {
                if tenant_mode != TenantMode::Multi {
                    return Err(ConfigError::Invalid("PROVISIONING"));
                }
                let secret = variables.required("PROVISION_SECRET")?;
                // Anything short is guessable over the network; refuse to boot with it.
                if secret.len() < 16 {
                    return Err(ConfigError::Invalid("PROVISION_SECRET"));
                }
                Provisioning::Enabled { secret }
            }
        };

        let app_env = match variables.one_of("APP_ENV", &["development", "production"])? {
            0 => AppEnvironment::Development,
            _ => AppEnvironment::Production,
        };

        let bind_addr = variables.required("BIND_ADDR")?;
        let auto_seed = variables.flag("AUTO_SEED")?;
        let seed_admin = SeedAdminConfig {
            name: variables.required("SEED_ADMIN_NAME")?,
            password: variables.required("SEED_ADMIN_PASSWORD")?,
        };
        // A publicly reachable instance must never come up with a well-known admin login.
        let publicly_reachable = bind_addr != "127.0.0.1" && bind_addr != "localhost";
        let password = seed_admin.password.as_str();
        if auto_seed
            && publicly_reachable
            && (password == "admin@1234"
                || password.starts_with("replace_with")
                || password.starts_with("change_this"))
        {
            return Err(ConfigError::Invalid("SEED_ADMIN_PASSWORD"));
        }

        let bucket = |burst: &'static str, refill: &'static str| {
            Ok::<_, ConfigError>(RateLimitBucket {
                burst: variables.non_negative(burst)?,
                refill_per_second: variables.non_negative(refill)?,
            })
        };
        let rate_limits = RateLimits {
            general: bucket("RATE_LIMIT_BURST", "RATE_LIMIT_REFILL_PER_SEC")?,
            sensitive: bucket(
                "RATE_LIMIT_SENSITIVE_BURST",
                "RATE_LIMIT_SENSITIVE_REFILL_PER_SEC",
            )?,
            live: bucket("RATE_LIMIT_LIVE_BURST", "RATE_LIMIT_LIVE_REFILL_PER_SEC")?,
        };

        let logging = LoggingConfig {
            filter: {
                let filter = variables.required("LOG_FILTER")?;
                tracing_subscriber::EnvFilter::try_new(&filter)
                    .map_err(|_| ConfigError::Invalid("LOG_FILTER"))?;
                filter
            },
            format: match variables.one_of("LOG_FORMAT", &["json", "text"])? {
                0 => LogFormat::Json,
                _ => LogFormat::Text,
            },
            body_cap_bytes: variables.parsed("LOG_BODY_CAP_BYTES")?,
            sql: match variables.one_of("LOG_SQL", &["off", "slow", "all"])? {
                0 => SqlLogging::Off,
                1 => SqlLogging::Slow,
                _ => SqlLogging::All,
            },
            http_bodies: variables.flag("LOG_HTTP_BODIES")?,
        };

        Ok(Self {
            database,
            jwt_secret,
            port: variables.parsed("PORT")?,
            bind_addr,
            jwt_expiry_hours: variables.parsed("JWT_EXPIRY_HOURS")?,
            document_server_url: variables.required("DOCUMENT_SERVER_URL")?,
            document_server_api_key,
            generated_documents_dir: variables.required("GENERATED_DOCUMENTS_DIR")?,
            return_window_days: variables.parsed("RETURN_WINDOW_DAYS")?,
            auto_seed,
            tenant_mode,
            cors_allowed_origins,
            identity_auth,
            provisioning,
            app_env,
            rate_limits,
            logging,
            installation_id: variables.required("INSTALLATION_ID")?,
            platform: variables.required("PLATFORM")?,
            seed_admin,
        })
    }
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

/// The database engine and exactly the settings that engine needs.
#[derive(Clone, PartialEq, Eq)]
pub enum Database {
    /// The desktop's single-shop SQLite file.
    Sqlite { url: String },
    /// MongoDB, in which a multi-tenant cloud keeps every shop.
    Mongo { uri: String, name: String },
}

/// Prints the engine and the database name, never the connection string, which carries
/// credentials.
impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Database::Sqlite { .. } => formatter.write_str("Sqlite"),
            Database::Mongo { name, .. } => write!(formatter, "Mongo({name})"),
        }
    }
}

/// Whether platform tokens from the identity service are accepted alongside local ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityAuth {
    Disabled,
    Enabled {
        jwks_url: String,
        /// The `iss` every platform token must carry.
        issuer: String,
        /// The identity tenant id of this shop. Set for a single-shop deployment, which accepts
        /// only that shop's tokens; `None` in multi-tenant mode, where the token's own tenant
        /// scopes every query.
        tenant_id: Option<String>,
    },
}

/// How log lines are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// JSON lines, which the desktop shell parses.
    Json,
    Text,
}

/// How the process logs, fixed at startup (the desktop shell can then switch parts at runtime).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingConfig {
    /// A `tracing` filter directive such as `info,tower_http=info`.
    pub filter: String,
    pub format: LogFormat,
    /// How many bytes of a request or response body are logged.
    pub body_cap_bytes: usize,
    pub sql: SqlLogging,
    pub http_bodies: bool,
}

/// Whether the identity service may register new shops through `POST /api/internal/provision`.
#[derive(Clone, PartialEq, Eq)]
pub enum Provisioning {
    Disabled,
    Enabled {
        /// The shared secret presented in `X-Provision-Secret`.
        secret: String,
    },
}

/// Prints only whether provisioning is on; the secret never reaches a log.
impl std::fmt::Debug for Provisioning {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provisioning::Disabled => formatter.write_str("Disabled"),
            Provisioning::Enabled { .. } => formatter.write_str("Enabled"),
        }
    }
}

/// One token bucket: how many requests it allows at once and how fast it refills.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateLimitBucket {
    pub burst: f64,
    pub refill_per_second: f64,
}

/// Per-client request limits, one bucket per class of route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateLimits {
    /// Every route.
    pub general: RateLimitBucket,
    /// The unauthenticated entry points (`/api/auth`, `/api/system`, `/api/internal`).
    pub sensitive: RateLimitBucket,
    /// Live calculations (`/api/calc`).
    pub live: RateLimitBucket,
}

/// The first administrator account that seeding creates.
#[derive(Clone, PartialEq, Eq)]
pub struct SeedAdminConfig {
    pub name: String,
    pub password: String,
}

/// Prints the name only; the password never reaches a log.
impl std::fmt::Debug for SeedAdminConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SeedAdminConfig")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Whether one deployment serves a single shop or many isolated tenants.
/// `Multi` requires MongoDB and makes every database handle tenant-scoped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantMode {
    Single,
    Multi,
}

/// A source of named configuration values: the process environment in production, a fixed map
/// in tests. Every variable the backend reads is read through here and every one is required.
pub struct Variables<'source> {
    lookup: &'source dyn Fn(&str) -> Option<String>,
}

impl<'source> Variables<'source> {
    /// Reads variables with `lookup`, which returns `None` for an unset one.
    #[must_use]
    pub fn new(lookup: &'source dyn Fn(&str) -> Option<String>) -> Self {
        Self { lookup }
    }

    /// The value of `name`, which must be set and not blank.
    pub fn required(&self, name: &'static str) -> Result<String, ConfigError> {
        match (self.lookup)(name) {
            Some(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
            _ => Err(ConfigError::Missing(name)),
        }
    }

    /// The value of `name`, which must be set; an empty value is allowed and means "none".
    pub fn required_allowing_empty(&self, name: &'static str) -> Result<String, ConfigError> {
        (self.lookup)(name)
            .map(|value| value.trim().to_string())
            .ok_or(ConfigError::Missing(name))
    }

    /// The value of `name` parsed as `T`.
    pub fn parsed<T: FromStr>(&self, name: &'static str) -> Result<T, ConfigError> {
        self.required(name)?
            .parse()
            .map_err(|_| ConfigError::Invalid(name))
    }

    /// The value of `name`, which must be exactly `true` or `false`.
    pub fn flag(&self, name: &'static str) -> Result<bool, ConfigError> {
        match self.required(name)?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(ConfigError::Invalid(name)),
        }
    }

    /// The value of `name`, which must be one of `choices`; returns the index of the match.
    pub fn one_of(&self, name: &'static str, choices: &[&str]) -> Result<usize, ConfigError> {
        let value = self.required(name)?;
        choices
            .iter()
            .position(|choice| *choice == value)
            .ok_or(ConfigError::Invalid(name))
    }

    /// A non-negative number, for rate limits.
    pub fn non_negative(&self, name: &'static str) -> Result<f64, ConfigError> {
        let value: f64 = self.parsed(name)?;
        if value.is_finite() && value >= 0.0 {
            Ok(value)
        } else {
            Err(ConfigError::Invalid(name))
        }
    }
}
