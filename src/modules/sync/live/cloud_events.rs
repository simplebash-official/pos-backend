use super::live_event::LiveEvent;

use axum::response::sse::Event;
use futures_util::{Stream, StreamExt, stream};

use serde_json::json;

use std::convert::Infallible;

use tokio::sync::broadcast;

/// The SSE body of `GET /api/sync/events`: `hello` with the latest position,
/// then one `change` per announced row (the caller's own writes left out),
/// or `resync` when this subscriber fell too far behind to be told each one.
/// `receiver` must be subscribed before `latest_seq` is read, so nothing
/// written in between is missed.
pub fn cloud_events(
    receiver: broadcast::Receiver<LiveEvent>,
    latest_seq: i64,
    caller_origin: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let hello = Event::default()
        .event("hello")
        .data(json!({ "latestSeq": latest_seq }).to_string());
    let changes = stream::unfold(
        (receiver, caller_origin),
        |(mut receiver, caller_origin)| async move {
            loop {
                let event = match receiver.recv().await {
                    Ok(event) if event.origin.as_deref() == Some(caller_origin.as_str()) => {
                        continue;
                    }
                    Ok(event) => Event::default()
                        .event("change")
                        .id(event.seq.to_string())
                        .data(serde_json::to_string(&event).unwrap_or_default()),
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        Event::default().event("resync").data("{}")
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                };
                return Some((Ok(event), (receiver, caller_origin)));
            }
        },
    );
    stream::once(async move { Ok(hello) }).chain(changes)
}
