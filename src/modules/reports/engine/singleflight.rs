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
