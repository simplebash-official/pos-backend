// The backend's view of the document-server's template list
// (`DocumentServerClient::list_templates`): refreshed on a timer so templates
// added without a restart show up, kept (stale) when a refresh fails so a
// document-server hiccup never stops printing, and kept per tenant (the
// document-server answers each tenant its own list). Uses a tiny controllable
// mock — no database needed.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    clients::document_server::DocumentServerClient,
    core::tenancy::{Tenant, with_tenant},
};

#[derive(Default)]
struct Mock {
    fetches: AtomicUsize,
    failing: AtomicBool,
    /// Tenant key (`""` = none) -> the list that tenant is served.
    templates: Mutex<std::collections::HashMap<String, Vec<Value>>>,
    /// The `X-Internal-Api-Key` of every list request.
    keys: Mutex<Vec<String>>,
}

impl Mock {
    fn set(&self, templates: Vec<Value>) {
        self.set_for("", templates);
    }

    fn set_for(&self, tenant: &str, templates: Vec<Value>) {
        self.templates
            .lock()
            .unwrap()
            .insert(tenant.to_string(), templates);
    }
}

fn template(name: &str, title: &str, kind: &str) -> Value {
    json!({
        "key": format!("tpl_{name}"),
        "name": name,
        "family": name,
        "version": 1,
        "type": "document",
        "kind": kind,
        "description": title,
        "status": "published",
        "scope": "global",
        "isDefault": false,
        "options": [],
        "sha256": "0".repeat(64),
        "minAppVersion": null,
        "dataSchema": { "type": "object", "x-document-kind": kind },
        "data": { "invoiceNumber": "SAMPLE" },
    })
}

async fn start(mock: Arc<Mock>) -> String {
    async fn list(State(mock): State<Arc<Mock>>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
        mock.fetches.fetch_add(1, Ordering::SeqCst);
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string()
        };
        mock.keys.lock().unwrap().push(header("X-Internal-Api-Key"));
        if mock.failing.load(Ordering::SeqCst) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": "boom", "code": "INTERNAL" })),
            );
        }
        let templates = mock
            .templates
            .lock()
            .unwrap()
            .get(&header("X-Tenant-Key"))
            .cloned()
            .unwrap_or_default();
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
    mock.set(vec![template("doc_temp_a", "A4 Invoice", "a4-invoice")]);
    let client = client(start(mock.clone()).await, Duration::from_secs(60));

    client.list_templates().await.unwrap();
    client.list_templates().await.unwrap();
    assert_eq!(mock.fetches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_expired_list_is_refetched_so_new_templates_appear_without_a_restart() {
    let mock = Arc::new(Mock::default());
    mock.set(vec![template("doc_temp_a", "A4 Invoice", "a4-invoice")]);
    let client = client(start(mock.clone()).await, Duration::ZERO);

    assert_eq!(client.list_templates().await.unwrap().len(), 1);

    mock.set(vec![
        template("doc_temp_a", "A4 Invoice", "a4-invoice"),
        template("doc_temp_b", "A4 Invoice Fancy", "a4-invoice"),
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
    mock.set(vec![template("doc_temp_a", "A4 Invoice", "a4-invoice")]);
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
async fn the_list_carries_family_version_kind_and_options_and_sends_the_internal_key() {
    let mock = Arc::new(Mock::default());
    let mut fancy = template("doc_temp_b", "A4 Invoice Fancy", "a4-invoice");
    fancy["family"] = json!("fam_fancy");
    fancy["version"] = json!(3);
    fancy["scope"] = json!("private");
    fancy["isDefault"] = json!(true);
    fancy["options"] =
        json!([{ "key": "showSign", "type": "toggle", "label": "Sign", "default": true }]);
    mock.set(vec![fancy]);
    let client = client(start(mock.clone()).await, Duration::from_secs(60));

    let list = client.list_templates().await.unwrap();
    assert_eq!(list.len(), 1);
    let fancy = &list[0];
    assert_eq!(
        (fancy.name.as_str(), fancy.family.as_str(), fancy.version),
        ("doc_temp_b", "fam_fancy", 3)
    );
    assert_eq!(fancy.kind, "a4-invoice");
    assert_eq!(fancy.title, "A4 Invoice Fancy");
    assert_eq!(fancy.key, "tpl_doc_temp_b");
    assert_eq!(fancy.scope, "private");
    assert!(fancy.is_default);
    assert_eq!(fancy.options[0].key, "showSign");
    assert_eq!(*mock.keys.lock().unwrap(), ["test-key"]);
}

#[tokio::test]
async fn each_tenant_is_served_and_cached_its_own_list() {
    let mock = Arc::new(Mock::default());
    mock.set_for(
        "tnt_a",
        vec![template("doc_temp_a", "Only A", "a4-invoice")],
    );
    mock.set_for(
        "tnt_b",
        vec![template("doc_temp_b", "Only B", "a4-invoice")],
    );
    let client = Arc::new(client(start(mock.clone()).await, Duration::from_secs(60)));

    let names = |tenant: &'static str| {
        let client = client.clone();
        async move {
            with_tenant(Tenant::id(tenant).unwrap(), async move {
                client
                    .list_templates()
                    .await
                    .unwrap()
                    .iter()
                    .map(|t| t.name.clone())
                    .collect::<Vec<_>>()
            })
            .await
        }
    };
    assert_eq!(names("tnt_a").await, ["doc_temp_a"]);
    assert_eq!(names("tnt_b").await, ["doc_temp_b"]);
    // Both are cached now, separately: no further fetch, no mix-up.
    assert_eq!(names("tnt_a").await, ["doc_temp_a"]);
    assert_eq!(names("tnt_b").await, ["doc_temp_b"]);
    assert_eq!(mock.fetches.load(Ordering::SeqCst), 2);
}
