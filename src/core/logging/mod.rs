// Structured logging for the backend: subscriber setup, the per-request
// logging middleware, secret redaction and the `log_event!` macro for
// business events.
//
// Two output modes, picked by `LOG_FORMAT`:
// - `text` (default, web/Docker): the human-readable `fmt` output as before.
// - `json` (set by the Tauri desktop shell): one JSON object per line on
//   stdout with a local-offset timestamp, which the shell ingests into the
//   unified desktop activity log. Contract: `docs/logging.md` in the
//   compose repo.

pub mod domain;
pub mod redact;
pub mod request;

use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::ChronoLocal;

/// Logging knobs read once from the environment. Env config is the one
/// place a default is allowed (see CLAUDE.md), and every one of these is
/// optional — the web deployment sets none of them.
#[derive(Debug, Clone)]
pub struct LogSettings {
    /// `LOG_FORMAT=json` → JSON lines on stdout.
    pub json: bool,
    /// `LOG_HTTP_BODIES` → log request/response bodies (redacted, capped).
    pub http_bodies: bool,
    /// `LOG_BODY_CAP_BYTES` → per-body cap when bodies are logged.
    pub body_cap_bytes: usize,
    /// `LOG_SQL` → `all` | `slow` | `off` (SQLite statement logging).
    pub sql: SqlLogging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlLogging {
    All,
    Slow,
    Off,
}

impl LogSettings {
    fn from_env() -> Self {
        let var = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|v| v.trim().to_ascii_lowercase())
        };
        LogSettings {
            json: var("LOG_FORMAT").as_deref() == Some("json"),
            http_bodies: matches!(
                var("LOG_HTTP_BODIES").as_deref(),
                Some("true" | "1" | "yes")
            ),
            body_cap_bytes: var("LOG_BODY_CAP_BYTES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(32 * 1024),
            sql: match var("LOG_SQL").as_deref() {
                Some("all") => SqlLogging::All,
                Some("off") => SqlLogging::Off,
                _ => SqlLogging::Slow,
            },
        }
    }
}

pub fn settings() -> &'static LogSettings {
    static SETTINGS: OnceLock<LogSettings> = OnceLock::new();
    SETTINGS.get_or_init(LogSettings::from_env)
}

/// Install the global subscriber. `default_filter` applies when `RUST_LOG`
/// is unset.
pub fn init(default_filter: &str) {
    let filter =
        EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));
    if settings().json {
        tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            // The full span list (not just the innermost span) keeps the
            // request span's `request_id` on lines emitted inside nested spans.
            .with_current_span(false)
            .with_span_list(true)
            .with_target(true)
            .with_timer(ChronoLocal::rfc_3339())
            .with_ansi(false)
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

/// Record a business event (sale completed, stock adjusted, user created…)
/// as `category=domain`. Call it from the service layer, after the change
/// succeeded, with the identifying keys as fields:
///
/// ```ignore
/// log_event!("sale.created", invoice_key = %invoice.key, total_cents = invoice.total, "Sale completed");
/// ```
///
/// The request's `request_id` / `user_id` are attached automatically from
/// the request span.
#[macro_export]
macro_rules! log_event {
    ($event:literal, $($rest:tt)+) => {
        ::tracing::info!(target: "domain", category = "domain", event = $event, $($rest)+)
    };
    ($event:literal) => {
        ::tracing::info!(target: "domain", category = "domain", event = $event, $event)
    };
}
