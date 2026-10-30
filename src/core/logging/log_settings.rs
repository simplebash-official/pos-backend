use super::sql_logging::SqlLogging;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Logging knobs. The three marked *runtime* can be changed while the process
/// runs, by a `log_mode` control line on stdin (see `control.rs`) — that is
/// how the desktop System Benchmark measures logging on vs off without a
/// restart. Env config is the one place a default is allowed (see CLAUDE.md),
/// and every variable is optional: the web deployment sets none of them.
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
    pub(super) fn from_env() -> Self {
        let var = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|v| v.trim().to_ascii_lowercase())
        };
        let sql = var("LOG_SQL").map_or(SqlLogging::Slow, |v| SqlLogging::parse(&v));
        LogSettings {
            json: var("LOG_FORMAT").as_deref() == Some("json"),
            body_cap_bytes: var("LOG_BODY_CAP_BYTES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(32 * 1024),
            enabled: AtomicBool::new(true),
            http_bodies: AtomicBool::new(matches!(
                var("LOG_HTTP_BODIES").as_deref(),
                Some("true" | "1" | "yes")
            )),
            sql: AtomicU8::new(sql.code()),
            sql_at_startup: sql,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn http_bodies(&self) -> bool {
        self.enabled() && self.http_bodies.load(Ordering::Relaxed)
    }

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
