use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;
use crate::core::config::Config;

use crate::modules;

use std::sync::Arc;

/// Shared application state injected into every handler via Axum's
/// `State` extractor. `Arc<Config>` because `Config` is read-only after
/// startup and cloned into every request's extensions; `Db` is
/// an internally-`Arc`'d handle (either Mongo or SQLite pool), so cloning it
/// per-request is cheap. `document_server` is behind its own `Arc` since
/// `DocumentServerClient` holds a `reqwest::Client` (already internally
/// `Arc`'d) plus a mutable template-key cache that must be shared, not
/// cloned, across requests.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub document_server: Arc<DocumentServerClient>,
    pub reports_engine: Arc<modules::reports::engine::AnalyticsEngine>,
}
