use super::cloud_events::cloud_events;
use super::live_event::LiveEvent;
use super::live_event_of::live_event_of;
use super::local_events::local_events;
use super::local_writes::local_writes;
use super::publish::publish;
use super::subscribe::subscribe;

use futures_util::StreamExt;
use mongodb::bson::doc;

use std::time::Duration;

#[tokio::test]
async fn subscribers_get_other_writers_changes_and_never_their_own() {
    let receiver = subscribe("t_live_1");
    let events = cloud_events(receiver, 7, "dev_a".to_string());
    tokio::pin!(events);
    // hello first
    assert!(events.next().await.is_some());

    publish(
        "t_live_1",
        LiveEvent {
            seq: 8,
            resource: "products".into(),
            origin: Some("dev_a".into()),
        },
    );
    publish(
        "t_live_1",
        LiveEvent {
            seq: 9,
            resource: "customers".into(),
            origin: Some("web:x".into()),
        },
    );
    let next = tokio::time::timeout(Duration::from_secs(1), events.next())
        .await
        .expect("an event")
        .expect("open");
    assert!(next.is_ok());
    // Another tenant's change is never seen.
    publish(
        "t_live_other",
        LiveEvent {
            seq: 1,
            resource: "products".into(),
            origin: None,
        },
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), events.next())
            .await
            .is_err()
    );
}

#[test]
fn change_log_rows_become_events() {
    let row = doc! {
        "tenant_id": "t1", "seq": 12_i64, "resource": "invoices", "origin_device_id": "dev_9",
    };
    let (tenant, event) = live_event_of(&row).unwrap();
    assert_eq!(tenant, "t1");
    assert_eq!(
        event,
        LiveEvent {
            seq: 12,
            resource: "invoices".into(),
            origin: Some("dev_9".into())
        }
    );
}

#[tokio::test]
async fn local_stream_announces_at_once_then_after_each_write() {
    let events = local_events();
    tokio::pin!(events);
    assert!(events.next().await.is_some(), "initial outbox event");
    local_writes().send_modify(|n| *n += 1);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .is_ok()
    );
}
