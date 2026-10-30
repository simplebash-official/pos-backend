use super::single_flight_group::SingleFlightGroup;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

// Tenant-scoped keys are distinct strings, so identical reports asked by two
// tenants run twice and each caller gets its own tenant's value; the same
// key still coalesces to a single run.
#[tokio::test]
async fn distinct_keys_never_share_a_result_but_equal_keys_coalesce() {
    let group = Arc::new(SingleFlightGroup::<String>::new());
    let runs = Arc::new(AtomicUsize::new(0));

    let call = |key: &'static str, value: &'static str| {
        let (group, runs) = (group.clone(), runs.clone());
        tokio::spawn(async move {
            group
                .work(key, || async {
                    runs.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    Ok(value.to_string())
                })
                .await
                .unwrap()
        })
    };

    let a1 = call("reports:tenant=a:feed:x", "A");
    let a2 = call("reports:tenant=a:feed:x", "A-dup");
    let b = call("reports:tenant=b:feed:x", "B");
    let (a1, a2, b) = (a1.await.unwrap(), a2.await.unwrap(), b.await.unwrap());

    assert_eq!(a1, a2, "same tenant key coalesces to one result");
    assert_eq!(b, "B", "another tenant never receives tenant A's result");
    assert_eq!(runs.load(Ordering::SeqCst), 2);
}
