use serde::Serialize;
use serde_json::Value;
use std::fmt::Display;
use std::future::Future;
use std::time::Instant;

/// Pull a compact identity out of a service result: its `key` (and `number`
/// / `invoiceNumber` style business numbers when present), or the item count
/// for list results.
pub(super) fn summarize<T: Serialize>(
    value: &T,
) -> (Option<String>, Option<String>, Option<usize>) {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => {
            let key = map.get("key").and_then(Value::as_str).map(str::to_string);
            let number = ["invoiceNumber", "number", "code", "sku"]
                .iter()
                .find_map(|k| map.get(*k))
                .and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });
            (key, number, None)
        }
        Ok(Value::Array(items)) => (None, None, Some(items.len())),
        _ => (None, None, None),
    }
}

/// Runs `body` and logs the business event `event` with its outcome.
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
