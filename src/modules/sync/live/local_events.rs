use super::local_writes::local_writes;

use axum::response::sse::Event;
use futures_util::{Stream, stream};

use std::convert::Infallible;

/// The SSE body of `GET /api/sync/local/events`: one `outbox` event at once
/// (anything already waiting) and one after each local write, coalesced
/// while the agent is busy.
pub fn local_events() -> impl Stream<Item = Result<Event, Infallible>> {
    let mut receiver = local_writes().subscribe();
    receiver.mark_changed();
    stream::unfold(receiver, |mut receiver| async move {
        receiver.changed().await.ok()?;
        let tick = *receiver.borrow_and_update();
        Some((
            Ok(Event::default().event("outbox").data(tick.to_string())),
            receiver,
        ))
    })
}
