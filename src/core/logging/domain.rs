// Business-event logging for service functions. Wrapping a mutating service
// body in `tracked` records one `category=domain` line per call: the
// outcome, how long it took, and the identifying `key` of what was created
// or changed (read from the returned value). The full request/response
// bodies are already in the `http` lines of the same `request_id`; this is
// the business-level summary ("sale.completed inv_… in 42 ms").
//
// ```ignore
// pub async fn complete_sale(state: &AppState, input: SaleInput) -> AppResult<Invoice> {
//     tracked("billing.sale_completed", async move {
//         /* existing body */
//     })
//     .await
// }
// ```

use std::fmt::Display;
use std::future::Future;
use std::time::Instant;

use serde::Serialize;
use serde_json::Value;

/// Pull a compact identity out of a service result: its `key` (and `number`
/// / `invoiceNumber` style business numbers when present), or the item count
/// for list results.
fn summarize<T: Serialize>(value: &T) -> (Option<String>, Option<String>, Option<usize>) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarize_reads_key_number_and_counts() {
        let (key, number, count) =
            summarize(&json!({"key": "inv_1", "invoiceNumber": "INV-0007", "total": 100}));
        assert_eq!(key.as_deref(), Some("inv_1"));
        assert_eq!(number.as_deref(), Some("INV-0007"));
        assert_eq!(count, None);

        assert_eq!(summarize(&vec![1, 2, 3]).2, Some(3));
        assert_eq!(summarize(&()), (None, None, None));
    }

    #[tokio::test]
    async fn tracked_passes_result_through() {
        let ok: Result<u8, String> = tracked("test.ok", async { Ok(7) }).await;
        assert_eq!(ok, Ok(7));
        let err: Result<u8, String> = tracked("test.err", async { Err("boom".to_string()) }).await;
        assert_eq!(err, Err("boom".to_string()));
    }
}
