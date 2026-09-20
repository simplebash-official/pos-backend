// High-performance request coalescing / single-flight group for Tokio.
// Prevents duplicate database aggregation runs when multiple concurrent requests
// ask for the same period report at the exact same instant.

use std::{collections::HashMap, future::Future, sync::Mutex};

use tokio::sync::broadcast;

use crate::core::error::{AppError, AppResult};

pub struct SingleFlightGroup<V>
where
    V: Clone + Send + 'static,
{
    calls: Mutex<HashMap<String, broadcast::Sender<AppResult<V>>>>,
}

impl<V> Default for SingleFlightGroup<V>
where
    V: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<V> SingleFlightGroup<V>
where
    V: Clone + Send + 'static,
{
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(HashMap::new()),
        }
    }

    /// Execute `work` for `key`. If another request for `key` is already in progress,
    /// wait for the in-progress execution and return a clone of its result.
    pub async fn work<F, Fut>(&self, key: &str, work: F) -> AppResult<V>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<V>>,
    {
        // Check if there is already an in-flight execution for `key`.
        let mut rx = {
            let mut calls = self.calls.lock().unwrap();
            if let Some(sender) = calls.get(key) {
                // Subscribe to existing in-flight task
                Some(sender.subscribe())
            } else {
                // We are the leader for this key
                let (tx, _) = broadcast::channel(1);
                calls.insert(key.to_string(), tx);
                None
            }
        };

        // If we are a subscriber, wait for the leader's result.
        if let Some(ref mut subscriber) = rx {
            match subscriber.recv().await {
                Ok(result) => return result,
                Err(_) => {
                    return Err(AppError::internal(
                        "SingleFlight leader worker dropped without sending a result",
                    ));
                }
            }
        }

        // We are the leader: execute the underlying computation.
        let result = work().await;

        // Clean up from the map and broadcast result to all concurrent subscribers.
        let sender = {
            let mut calls = self.calls.lock().unwrap();
            calls.remove(key)
        };

        if let Some(tx) = sender {
            // It's ok if there are 0 subscribers
            let _ = tx.send(result.clone());
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
