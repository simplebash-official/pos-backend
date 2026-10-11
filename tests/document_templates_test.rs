// Document designs: the shop picks one template per kind of document (A4
// invoice, receipt, credit note) from whatever the document-server publishes,
// and a single print can override it. The mock document-server (see
// `common::mock_templates_json`) offers two A4 designs, a retired (inactive)
// one, two receipts and two credit notes, and its render echoes which template
// printed and whether a `logoUrl` was still in the payload.
//
// Settings behaviour runs on SQLite (no Atlas needed); the print flow and the
// per-shop isolation run against throwaway Atlas databases.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
use tower::ServiceExt;

const CLASSIC: &str = "doc_temp_vEf0Y7jQHQj2rIuO";
const MODERN: &str = "doc_temp_ZahcoYg8xCQjuc8c";
const RETIRED: &str = "doc_temp_RetiredInvoice00";
const COMPACT_RECEIPT: &str = "doc_temp_CompactReceipt00";
const ANALYTICS: &str = "doc_temp_An1yT1csRep0rtV1";

async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = builder
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

async fn call_json(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, bytes) = call(router, method, uri, Some(token), body).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn token(app: &common::TestApp, role: Role) -> String {
    common::mint_token(&app.config, Some(role), roles::default_permissions(role))
}

fn kind<'a>(settings: &'a Value, slug: &str) -> &'a Value {
    settings["data"]["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == slug)
        .unwrap_or_else(|| panic!("no kind {slug} in {settings}"))
}

fn option_names(kind: &Value) -> Vec<String> {
    kind["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

const SETTINGS: &str = "/api/settings/document-templates";

// ---------------------------------------------------------------------------
// Settings endpoints (SQLite)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lists_each_kind_with_its_active_designs_standard_design_first() {
    let app = common::spawn_app_sqlite().await;
    let (status, body) = call_json(
        &app.router,
        "GET",
        SETTINGS,
        &token(&app, Role::Staff),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let slugs: Vec<&str> = body["data"]["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["a4-invoice", "thermal-receipt", "credit-note"]);

    let invoice = kind(&body, "a4-invoice");
    // The retired design is never offered; the standard design leads.
    assert_eq!(option_names(invoice), [CLASSIC, MODERN]);
    assert_eq!(invoice["templates"][0]["isDefault"], true);
    assert_eq!(invoice["templates"][0]["title"], "A4 Invoice");
    assert_eq!(invoice["templates"][1]["isDefault"], false);
    assert_eq!(invoice["selected"], Value::Null);
    assert_eq!(invoice["effective"], CLASSIC);

    // The analytics report is not something a shop chooses.
    assert!(
        body["data"]["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["kind"] != "analytics-report")
    );
}

#[tokio::test]
async fn an_admin_can_choose_and_clear_a_design_and_it_is_remembered() {
    let app = common::spawn_app_sqlite().await;
    let admin = token(&app, Role::Admin);

    let (status, body) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": MODERN } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(kind(&body, "a4-invoice")["selected"], MODERN);
    assert_eq!(kind(&body, "a4-invoice")["effective"], MODERN);

    // Remembered across requests, for every signed-in user; other kinds untouched.
    let (_, again) = call_json(
        &app.router,
        "GET",
        SETTINGS,
        &token(&app, Role::Staff),
        None,
    )
    .await;
    assert_eq!(kind(&again, "a4-invoice")["selected"], MODERN);
    assert_eq!(kind(&again, "thermal-receipt")["selected"], Value::Null);

    // `null` returns the kind to the standard design.
    let (status, cleared) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": null } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert_eq!(kind(&cleared, "a4-invoice")["selected"], Value::Null);
    assert_eq!(kind(&cleared, "a4-invoice")["effective"], CLASSIC);
}

#[tokio::test]
async fn only_an_admin_can_change_the_designs() {
    let app = common::spawn_app_sqlite().await;
    let (status, _) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &token(&app, Role::Staff),
        Some(json!({ "selections": { "a4-invoice": MODERN } })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, unchanged) = call_json(
        &app.router,
        "GET",
        SETTINGS,
        &token(&app, Role::Staff),
        None,
    )
    .await;
    assert_eq!(kind(&unchanged, "a4-invoice")["selected"], Value::Null);
}

#[tokio::test]
async fn bad_choices_are_rejected_and_change_nothing() {
    let app = common::spawn_app_sqlite().await;
    let admin = token(&app, Role::Admin);

    // Not a design, a retired design, a design of another kind.
    for bad in ["doc_temp_nope", RETIRED, COMPACT_RECEIPT, ANALYTICS] {
        let (status, body) = call_json(
            &app.router,
            "PUT",
            SETTINGS,
            &admin,
            Some(json!({ "selections": { "a4-invoice": bad } })),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert_eq!(body["code"], "TEMPLATE_UNAVAILABLE", "{bad}");
    }

    // A kind that does not exist, and a mixed update where one half is bad.
    let (status, _) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "invoice-ish": MODERN } })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": MODERN, "thermal-receipt": "doc_temp_nope" } })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, after) = call_json(&app.router, "GET", SETTINGS, &admin, None).await;
    assert_eq!(kind(&after, "a4-invoice")["selected"], Value::Null);
    assert_eq!(kind(&after, "thermal-receipt")["selected"], Value::Null);
}

#[tokio::test]
async fn preview_renders_a_designs_own_sample_without_its_logo_url() {
    let app = common::spawn_app_sqlite().await;
    let staff = token(&app, Role::Staff);

    let (status, pdf) = call(
        &app.router,
        "GET",
        &format!("{SETTINGS}/{MODERN}/preview"),
        Some(&staff),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.starts_with("%PDF-"), "{text}");
    assert!(text.contains("tpl_a4_invoice_modern"), "{text}");
    // The sample carries a remote logo; a preview must not depend on the network.
    assert!(text.contains("logoUrl=false"), "{text}");
}

#[tokio::test]
async fn preview_only_serves_designs_a_shop_could_choose() {
    let app = common::spawn_app_sqlite().await;
    let staff = token(&app, Role::Staff);
    for name in ["doc_temp_nope", RETIRED, ANALYTICS] {
        let (status, _) = call(
            &app.router,
            "GET",
            &format!("{SETTINGS}/{name}/preview"),
            Some(&staff),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name}");
    }
}

// ---------------------------------------------------------------------------
// Printing follows the chosen design (Atlas, throwaway database)
// ---------------------------------------------------------------------------

async fn make_sale(app: &common::TestApp, cashier: &str) -> String {
    let (status, body) = call_json(
        &app.router,
        "POST",
        "/api/billing/sales",
        cashier,
        Some(json!({
            "staff": { "cashierName": "Nimal Perera" },
            "items": [{
                "name": "Widget",
                "unitPriceCents": 1000,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail",
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false },
            "shopProfileSnapshot": { "tradingName": "TechFix Repairs" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"]["invoice"]["key"].as_str().unwrap().to_string()
}

async fn print(
    app: &common::TestApp,
    cashier: &str,
    invoice_key: &str,
    document: &str,
    query: &str,
) -> (StatusCode, String) {
    let (status, bytes) = call(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}/documents/{document}{query}"),
        Some(cashier),
        None,
    )
    .await;
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn rows(app: &common::TestApp, invoice_key: &str) -> Vec<Document> {
    use futures_util::TryStreamExt;
    app.db
        .collection::<Document>("generated_documents")
        .find(doc! { "entity_key": invoice_key })
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}

#[tokio::test]
async fn printing_follows_the_shops_design_and_caches_each_design_separately() {
    let app = common::spawn_app_isolated().await;
    let cashier = token(&app, Role::Staff);
    let admin = token(&app, Role::Admin);
    let invoice = make_sale(&app, &cashier).await;

    // Nothing chosen: the standard design, under the historical cache key.
    let (status, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.contains("tpl_a4_invoice "), "{pdf}");

    // The admin picks Modern: the same invoice now prints in it.
    let (status, _) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": MODERN } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.contains("tpl_a4_invoice_modern"), "{pdf}");

    let stored = rows(&app, &invoice).await;
    assert_eq!(stored.len(), 2, "one cached PDF per design: {stored:?}");
    let modern = stored
        .iter()
        .find(|r| r.get_str("document_type").ok() == Some("a4-invoice--doc_temp_ZahcoYg8xCQjuc8c"))
        .expect("the modern print is cached under its own key");
    assert_eq!(modern.get_str("template_name").unwrap(), "a4-invoice");
    assert_eq!(
        modern.get_str("template_key").unwrap(),
        "tpl_a4_invoice_modern"
    );

    // Back to the standard design: served from the cache, not re-rendered.
    call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": null } })),
    )
    .await;
    let (_, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert!(pdf.contains("tpl_a4_invoice "), "{pdf}");
    assert_eq!(rows(&app, &invoice).await.len(), 2, "no extra render");

    app.db.drop().await.ok();
}

#[tokio::test]
async fn one_print_can_use_another_design_without_changing_the_shops_choice() {
    let app = common::spawn_app_isolated().await;
    let cashier = token(&app, Role::Staff);
    let invoice = make_sale(&app, &cashier).await;

    let (status, pdf) = print(
        &app,
        &cashier,
        &invoice,
        "a4-invoice",
        &format!("?template={MODERN}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.contains("tpl_a4_invoice_modern"), "{pdf}");

    // The shop's choice is unchanged: the next plain print is the standard design.
    let (_, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert!(pdf.contains("tpl_a4_invoice "), "{pdf}");

    // Receipts take it too; the paper width stays part of the cache key.
    let (status, pdf) = print(
        &app,
        &cashier,
        &invoice,
        "thermal-receipt",
        &format!("?paperWidthMm=58&template={COMPACT_RECEIPT}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.contains("tpl_thermal_receipt_compact"), "{pdf}");
    assert!(
        rows(&app, &invoice)
            .await
            .iter()
            .any(|r| r.get_str("document_type").ok()
                == Some("thermal-receipt-58mm--doc_temp_CompactReceipt00"))
    );

    // A design that does not exist, was retired, or belongs to another kind of
    // document is a clear error rather than a silent switch.
    for bad in ["doc_temp_nope", RETIRED, COMPACT_RECEIPT, "..%2Fetc"] {
        let (status, body) = print(
            &app,
            &cashier,
            &invoice,
            "a4-invoice",
            &format!("?template={bad}"),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert!(body.contains("TEMPLATE_UNAVAILABLE"), "{bad}: {body}");
    }

    app.db.drop().await.ok();
}

#[tokio::test]
async fn a_removed_design_quietly_falls_back_to_the_standard_design() {
    let app = common::spawn_app_isolated().await;
    let cashier = token(&app, Role::Staff);
    let invoice = make_sale(&app, &cashier).await;

    // A choice made when the design still existed; the settings API would
    // refuse it now, so write it as it would have been stored.
    let now = mongodb::bson::DateTime::now();
    app.db
        .collection::<Document>("document_template_settings")
        .insert_one(doc! {
            "key": "main",
            "selections": { "a4-invoice": "doc_temp_removed_long_ago" },
            "version": 1,
            "created_at": now,
            "updated_at": now,
        })
        .await
        .unwrap();

    let (status, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert_eq!(status, StatusCode::OK, "a shop must still be able to print");
    assert!(pdf.contains("tpl_a4_invoice "), "{pdf}");

    // Settings reports it honestly: nothing selected, standard design in use.
    let (_, settings) = call_json(&app.router, "GET", SETTINGS, &cashier, None).await;
    assert_eq!(kind(&settings, "a4-invoice")["selected"], Value::Null);
    assert_eq!(kind(&settings, "a4-invoice")["effective"], CLASSIC);

    app.db.drop().await.ok();
}

#[tokio::test]
async fn each_shop_has_its_own_design_choice() {
    let app = common::spawn_app_multi_tenant().await;
    let admin_of = |tenant: &str| {
        common::mint_token_for_tenant(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
            Some(tenant),
        )
    };
    let (shop_a, shop_b) = (admin_of("tnt_shop_a"), admin_of("tnt_shop_b"));

    let (status, body) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &shop_a,
        Some(json!({ "selections": { "a4-invoice": MODERN } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (_, a) = call_json(&app.router, "GET", SETTINGS, &shop_a, None).await;
    let (_, b) = call_json(&app.router, "GET", SETTINGS, &shop_b, None).await;
    assert_eq!(kind(&a, "a4-invoice")["selected"], MODERN);
    assert_eq!(
        kind(&b, "a4-invoice")["selected"],
        Value::Null,
        "shop B must not inherit shop A's design"
    );
    assert_eq!(kind(&b, "a4-invoice")["effective"], CLASSIC);

    app.db.drop().await.ok();
}
