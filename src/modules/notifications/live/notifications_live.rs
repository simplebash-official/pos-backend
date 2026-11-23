use crate::core::tenancy::{Tenant, current_tenant};
use crate::modules::sync::live as sync_live;
use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use axum::response::sse::Event;
use futures_util::{Stream, StreamExt, stream};
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Mutex, OnceLock};
use tokio::sync::broadcast;

/// Channel of a single-shop deployment (desktop, self-hosted), which has no tenant id.
const SINGLE_SHOP_CHANNEL: &str = "single-shop";

/// Announcements buffered per shop before a slow subscriber is told to reload anyway.
const CHANNEL_CAPACITY: usize = 64;

/// Wire names of the two synced resources; a cloud change of either is announced.
const RESOURCES: [&str; 2] = ["notifications", "notificationReads"];

tokio::task_local! {
    /// Shops whose notifications the running write request changed.
    static CHANGED: RefCell<Vec<String>>;
}

/// The channel of the shop in scope; `None` when no shop is (an unauthenticated request in a
/// multi-shop deployment).
fn channel_in_scope() -> Option<String> {
    match current_tenant() {
        Some(Tenant::Id(id)) => Some(id.to_string()),
        Some(Tenant::Global) | None => Some(SINGLE_SHOP_CHANNEL.to_string()),
        Some(Tenant::Deny) => None,
    }
}

/// Records that the current request changed the shop's notifications. Announced by
/// [`announce_after_write`] once the request has succeeded, so listeners never reload before the
/// write is committed. Outside a write request (the retention worker) nothing is announced.
pub(crate) fn mark_changed() {
    let Some(channel) = channel_in_scope() else {
        return;
    };
    // `try_with` fails only outside a write request, where there is nobody to tell.
    let _outside_request = CHANGED.try_with(|changed| {
        let mut changed = changed.borrow_mut();
        if !changed.contains(&channel) {
            changed.push(channel);
        }
    });
}

/// Middleware: runs each write request with an empty change list and, when it succeeded,
/// announces the shops whose notifications it changed.
pub async fn announce_after_write(request: Request, next: Next) -> Response {
    if !matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return next.run(request).await;
    }
    let (response, changed) = CHANGED
        .scope(RefCell::new(Vec::new()), async {
            let response = next.run(request).await;
            (response, CHANGED.with(|changed| changed.take()))
        })
        .await;
    if response.status().is_success() {
        for channel in changed {
            send(&channel);
        }
    }
    response
}

fn channels() -> &'static Mutex<HashMap<String, broadcast::Sender<()>>> {
    static CHANNELS: OnceLock<Mutex<HashMap<String, broadcast::Sender<()>>>> = OnceLock::new();
    CHANNELS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn send(channel: &str) {
    let mut map = channels()
        .lock()
        .expect("notification channel registry mutex poisoned");
    // Nobody listening any more: drop the channel.
    if let Some(sender) = map.get(channel)
        && sender.send(()).is_err()
    {
        map.remove(channel);
    }
}

fn subscribe(channel: &str) -> broadcast::Receiver<()> {
    let mut map = channels()
        .lock()
        .expect("notification channel registry mutex poisoned");
    map.entry(channel.to_string())
        .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
        .subscribe()
}

/// The SSE body of `GET /notifications/events` for the shop in scope: `hello` on connect, then
/// `changed` whenever the shop's notifications or read marks change (on this server, or on any
/// other through the cloud change log). Carries no data: the client reloads what it shows.
/// `None` when no shop is in scope.
pub(crate) fn events() -> Option<impl Stream<Item = Result<Event, Infallible>>> {
    let channel = channel_in_scope()?;
    let local = stream::unfold(subscribe(&channel), |mut receiver| async move {
        match receiver.recv().await {
            // A lagged receiver still has to reload once.
            Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => Some(((), receiver)),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    });
    // Only a cloud shop has a change log; elsewhere this receiver simply never fires.
    let cloud = stream::unfold(sync_live::subscribe(&channel), |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) if RESOURCES.contains(&event.resource.as_str()) => {
                    return Some(((), receiver));
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => return Some(((), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    let hello = Event::default().event("hello").data("{}");
    let changes =
        stream::select(local, cloud).map(|()| Ok(Event::default().event("changed").data("{}")));
    Some(stream::once(async move { Ok(hello) }).chain(changes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_change_is_announced_only_after_a_successful_write() {
        let mut receiver = subscribe(SINGLE_SHOP_CHANNEL);
        let changed = CHANGED
            .scope(RefCell::new(Vec::new()), async {
                mark_changed();
                mark_changed();
                CHANGED.with(|changed| changed.take())
            })
            .await;
        assert_eq!(
            changed,
            vec![SINGLE_SHOP_CHANNEL.to_string()],
            "marked once per shop"
        );
        assert!(
            receiver.try_recv().is_err(),
            "nothing is sent before the request ends"
        );
        send(SINGLE_SHOP_CHANNEL);
        assert!(receiver.try_recv().is_ok());
    }

    #[test]
    fn marking_outside_a_request_is_a_no_op() {
        mark_changed();
    }
}
