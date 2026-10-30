use super::summarize::summarize;
use serde::Serialize;

use std::fmt::Display;
use std::future::Future;
use std::time::Instant;

pub async fn tracked<T, E, F>(event: &'static str, body: F) -> Result<T, E>
where
    T: Serialize,
    E: Display,
    F: Future<Output = Result<T, E>>,
{
    let started = Instant::now();
    let result = body.await;
    let duration_ms = started.elapsed().as_millis() as u64;
    match &result {
        Ok(value) => {
            let (key, number, count) = summarize(value);
            tracing::info!(
                target: "domain",
                category = "domain",
                event,
                outcome = "ok",
                key = key.as_deref(),
                number = number.as_deref(),
                count,
                duration_ms,
                "{event}"
            );
        }
        Err(err) => {
            tracing::warn!(
                target: "domain",
                category = "domain",
                event,
                outcome = "failed",
                error = %err,
                duration_ms,
                "{event} failed: {err}"
            );
        }
    }
    result
}
