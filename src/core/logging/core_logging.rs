use crate::core::config::{LogFormat, LoggingConfig};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::ChronoLocal;

/// Installs the logging settings and the global subscriber from the configuration.
pub fn init(config: &LoggingConfig) {
    let filter = EnvFilter::try_new(&config.filter)
        .expect("LOG_FILTER was validated when the configuration loaded");
    if install(config).json {
        use tracing_subscriber::Layer;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            // The full span list (not just the innermost span) keeps the
            // request span's `request_id` on lines emitted inside nested spans.
            .with_current_span(false)
            .with_span_list(true)
            .with_target(true)
            .with_timer(ChronoLocal::rfc_3339())
            .with_ansi(false)
            // Checked per event, so a `log_mode` control line takes effect
            // immediately and costs nothing when logging is on.
            .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
                let settings = settings();
                if !settings.enabled() {
                    return false;
                }
                if meta.target().starts_with("sqlx::query") {
                    return match settings.sql() {
                        SqlLogging::All => true,
                        // sqlx logs slow statements at WARN, the rest at INFO.
                        SqlLogging::Slow => *meta.level() <= tracing::Level::WARN,
                        SqlLogging::Off => false,
                    };
                }
                true
            }));
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

/// Logging knobs. The three marked *runtime* can be changed while the process
/// runs, by a `log_mode` control line on stdin (see `control.rs`) — that is
/// how the desktop System Benchmark measures logging on vs off without a
///
#[derive(Debug)]
pub struct LogSettings {
    /// `LOG_FORMAT=json` → JSON lines on stdout. Startup only.
    pub json: bool,
    /// `LOG_BODY_CAP_BYTES` → per-body cap when bodies are logged. Startup only.
    pub body_cap_bytes: usize,
    /// *Runtime*: record anything at all.
    pub(super) enabled: AtomicBool,
    /// *Runtime*: `LOG_HTTP_BODIES` → log request/response bodies.
    pub(super) http_bodies: AtomicBool,
    /// *Runtime*: `LOG_SQL` → SQLite statement logging.
    pub(super) sql: AtomicU8,
    /// Whether sqlx was wired for statement logging at connect time; when the
    /// process started with `LOG_SQL=off` no runtime change can turn it on.
    pub sql_at_startup: SqlLogging,
}

impl LogSettings {
    /// The settings a process starts with; the runtime ones can change afterwards.
    pub(super) fn from_config(config: &LoggingConfig) -> Self {
        LogSettings {
            json: config.format == LogFormat::Json,
            body_cap_bytes: config.body_cap_bytes,
            enabled: AtomicBool::new(true),
            http_bodies: AtomicBool::new(config.http_bodies),
            sql: AtomicU8::new(config.sql.code()),
            sql_at_startup: config.sql,
        }
    }

    /// Whether anything is logged at all.
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Whether request and response bodies are logged.
    pub fn http_bodies(&self) -> bool {
        self.enabled() && self.http_bodies.load(Ordering::Relaxed)
    }

    /// The current SQL statement logging level.
    pub fn sql(&self) -> SqlLogging {
        SqlLogging::from_code(self.sql.load(Ordering::Relaxed))
    }

    /// Apply a runtime change (from a stdin control line).
    pub fn apply(&self, enabled: bool, http_bodies: bool, sql: SqlLogging) {
        self.enabled.store(enabled, Ordering::Relaxed);
        self.http_bodies.store(http_bodies, Ordering::Relaxed);
        self.sql.store(sql.code(), Ordering::Relaxed);
    }
}

static SETTINGS: OnceLock<LogSettings> = OnceLock::new();

/// Installs the process-wide logging settings from the configuration. The first call wins; a
/// later one (several test apps in one process) returns the settings already installed.
pub fn install(config: &LoggingConfig) -> &'static LogSettings {
    SETTINGS.get_or_init(|| LogSettings::from_config(config))
}

/// The process-wide logging settings.
///
/// # Panics
///
/// When `install` has not run: the settings come from the configuration, so they exist only
/// once it has been loaded.
pub fn settings() -> &'static LogSettings {
    SETTINGS
        .get()
        .expect("logging settings are installed from the configuration at startup")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How much SQLite statement logging to emit.
pub enum SqlLogging {
    All,
    Slow,
    Off,
}

impl SqlLogging {
    #[must_use]
    /// Reads a level name; an unknown name means the default level.
    pub fn parse(raw: &str) -> SqlLogging {
        match raw.trim().to_ascii_lowercase().as_str() {
            "all" => SqlLogging::All,
            "off" => SqlLogging::Off,
            _ => SqlLogging::Slow,
        }
    }

    pub(super) fn code(self) -> u8 {
        match self {
            SqlLogging::Off => 0,
            SqlLogging::Slow => 1,
            SqlLogging::All => 2,
        }
    }

    pub(super) fn from_code(code: u8) -> SqlLogging {
        match code {
            0 => SqlLogging::Off,
            2 => SqlLogging::All,
            _ => SqlLogging::Slow,
        }
    }
}
