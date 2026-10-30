use super::settings::settings;
use super::sql_logging::SqlLogging;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::ChronoLocal;

/// Install the global subscriber. `default_filter` applies when `RUST_LOG`
/// is unset.
pub fn init(default_filter: &str) {
    let filter =
        EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));
    if settings().json {
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
