use super::current_origin::CURRENT_ORIGIN;

use std::future::Future;
use std::sync::Arc;

/// Runs `fut` with `origin` as the writer of every synced document it touches.
pub async fn with_origin<F: Future>(origin: impl Into<Arc<str>>, fut: F) -> F::Output {
    CURRENT_ORIGIN.scope(origin.into(), fut).await
}
