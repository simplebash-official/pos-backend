// The backend's view of the document-server's template list
// (`DocumentServerClient::list_templates`): refreshed on a timer so templates
// added without a restart show up, kept (stale) when a refresh fails so a
// document-server hiccup never stops printing, and limited to *active*
// templates. Uses a tiny controllable mock — no database needed.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};
use simplebash_pos_backend::clients::document_server::DocumentServerClient;

#[derive(Default)]
struct Mock {
    fetches: AtomicUsize,
    failing: AtomicBool,
    templates: Mutex<Vec<Value>>,
}

impl Mock {
    fn set(&self, templates: Vec<Value>) {
        *self.templates.lock().unwrap() = templates;
    }
}

fn template(name: &str, title: &str, active: bool, kind: Option<&str>) -> Value {
    let mut schema = json!({ "type": "object" });
    if let Some(kind) = kind {
        schema["x-document-kind"] = json!(kind);
    }
    json!({
        "key": format!("tpl_{name}"),
        "name": name,
        "description": title,
        "isActive": active,
        "dataSchema": schema,
        "data": { "invoiceNumber": "SAMPLE" },
    })
}

async fn start(mock: Arc<Mock>) -> String {
    async fn list(State(mock): State<Arc<Mock>>) -> (StatusCode, Json<Value>) {
        mock.fetches.fetch_add(1, Ordering::SeqCst);
        if mock.failing.load(Ordering::SeqCst) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": "boom", "code": "INTERNAL" })),
            );
        }
        let templates = mock.templates.lock().unwrap().clone();
        (
            StatusCode::OK,
            Json(json!({ "success": true, "message": "ok", "data": { "templates": templates } })),
        )
    }
    let router = Router::new()
        .route("/api/templates", get(list))
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });
    format!("http://{addr}")
}

fn client(url: String, ttl: Duration) -> DocumentServerClient {
    DocumentServerClient::new(url, "test-key".to_string()).with_cache_ttl(ttl)
}

#[tokio::test]
async fn the_list_is_cached_until_it_expires() {
    let mock = Arc::new(Mock::default());
    mock.set(vec![template(
        "doc_temp_a",
        "A4 Invoice",
        true,
        Some("a4-invoice"),
    )]);
    let client = client(start(mock.clone()).await, Duration::from_secs(60));

    client.list_templates().await.unwrap();
    client.list_templates().await.unwrap();
    client.find_by_name("doc_temp_a").await.unwrap();
    assert_eq!(mock.fetches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_expired_list_is_refetched_so_new_templates_appear_without_a_restart() {
    let mock = Arc::new(Mock::default());
    mock.set(vec![template(
        "doc_temp_a",
        "A4 Invoice",
        true,
        Some("a4-invoice"),
    )]);
    let client = client(start(mock.clone()).await, Duration::ZERO);

    assert_eq!(client.list_templates().await.unwrap().len(), 1);

    mock.set(vec![
        template("doc_temp_a", "A4 Invoice", true, Some("a4-invoice")),
        template("doc_temp_b", "A4 Invoice Fancy", true, Some("a4-invoice")),
    ]);
    let names: Vec<String> = client
        .list_templates()
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(names, ["doc_temp_a", "doc_temp_b"]);
    assert_eq!(mock.fetches.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_failed_refresh_keeps_serving_the_previous_list() {
    let mock = Arc::new(Mock::default());
    mock.set(vec![template(
        "doc_temp_a",
        "A4 Invoice",
        true,
        Some("a4-invoice"),
    )]);
    let client = client(start(mock.clone()).await, Duration::ZERO);
    client.list_templates().await.unwrap();

    mock.failing.store(true, Ordering::SeqCst);
    let stale = client
        .list_templates()
        .await
        .expect("a hiccup must not stop printing");
    assert_eq!(stale.len(), 1);
    assert_eq!(
        mock.fetches.load(Ordering::SeqCst),
        2,
        "it did try to refresh"
    );

    // ...and recovers on its own once the document-server is back.
    mock.failing.store(false, Ordering::SeqCst);
    mock.set(Vec::new());
    assert!(client.list_templates().await.unwrap().is_empty());
}

#[tokio::test]
async fn with_no_previous_list_a_failure_is_an_error() {
    let mock = Arc::new(Mock::default());
    mock.failing.store(true, Ordering::SeqCst);
    let client = client(start(mock).await, Duration::from_secs(60));
    assert!(client.list_templates().await.is_err());
}

#[tokio::test]
async fn only_active_templates_are_listed_and_the_kind_comes_from_the_schema() {
    let mock = Arc::new(Mock::default());
    mock.set(vec![
        template("doc_temp_a", "A4 Invoice", true, Some("a4-invoice")),
        template(
            "doc_temp_old",
            "A4 Invoice Retired",
            false,
            Some("a4-invoice"),
        ),
        template("doc_temp_plain", "No Marker", true, None),
    ]);
    let client = client(start(mock).await, Duration::from_secs(60));

    let list = client.list_templates().await.unwrap();
    let names: Vec<&str> = list.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["doc_temp_a", "doc_temp_plain"]);
    assert_eq!(list[0].kind.as_deref(), Some("a4-invoice"));
    assert_eq!(list[0].title, "A4 Invoice");
    assert_eq!(list[0].key, "tpl_doc_temp_a");
    assert_eq!(list[1].kind, None);

    assert!(client.find_by_name("doc_temp_old").await.unwrap().is_none());
    assert!(client.find_by_name("doc_temp_a").await.unwrap().is_some());
}
