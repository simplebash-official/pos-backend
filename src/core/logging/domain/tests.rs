use super::logging_domain::summarize;
use super::logging_domain::tracked;

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
