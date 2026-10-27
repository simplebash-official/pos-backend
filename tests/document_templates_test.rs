// Document designs: the shop picks one design family per kind of document (A4
// invoice, receipt, credit note) from whatever the document-server publishes,
// sets the options a design offers, and a single print can override the
// choice. The mock document-server (see `common::mock_templates_json`) offers
// two bundled A4 designs plus a custom one with options, two receipts and two
// credit notes, and its render echoes which template printed and the payload.
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
const CLIENT: &str = "fam_ClientInvoice00";
const CLIENT_NAME: &str = "doc_temp_ClientInvoiceV2";
const GONE: &str = "fam_RevokedLongAgo000";
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
    kind["families"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["family"].as_str().unwrap().to_string())
        .collect()
}

const SETTINGS: &str = "/api/settings/document-templates";

// ---------------------------------------------------------------------------
// Settings endpoints (SQLite)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lists_each_kind_with_its_designs_standard_design_first() {
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
    assert_eq!(
        slugs,
        [
            "a4-invoice",
            "thermal-receipt",
            "credit-note",
            "quotation",
            "z-report"
        ]
    );

    let invoice = kind(&body, "a4-invoice");
    // The standard design leads, then the rest by title (the custom design
    // carries its latest version and its options).
    assert_eq!(option_names(invoice), [CLASSIC, MODERN, CLIENT]);
    assert_eq!(invoice["families"][0]["isDefault"], true);
    assert_eq!(invoice["families"][0]["title"], "A4 Invoice");
    assert_eq!(invoice["families"][1]["isDefault"], false);
    assert_eq!(invoice["families"][2]["version"], 2);
    assert_eq!(invoice["families"][2]["options"][0]["key"], "showSign");
    assert_eq!(
        invoice["families"][2]["options"][1]["choices"][1]["value"],
        "red"
    );
    assert_eq!(invoice["selected"], Value::Null);
    assert_eq!(invoice["effective"], CLASSIC);
    // Option values in force: the design's declared defaults until saved.
    assert_eq!(
        invoice["optionValues"][CLIENT],
        json!({ "showSign": true, "accent": "blue" })
    );
    assert!(invoice["optionValues"].get(CLASSIC).is_none());

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
        Some(json!({ "selections": { "a4-invoice": MODERN }, "options": {} })),
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
        Some(json!({ "selections": { "a4-invoice": null }, "options": {} })),
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
        Some(json!({ "selections": { "a4-invoice": MODERN }, "options": {} })),
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

    // Not a design, a revoked design, a design of another kind.
    for bad in ["doc_temp_nope", GONE, COMPACT_RECEIPT, ANALYTICS] {
        let (status, body) = call_json(
            &app.router,
            "PUT",
            SETTINGS,
            &admin,
            Some(json!({ "selections": { "a4-invoice": bad }, "options": {} })),
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
        Some(json!({ "selections": { "invoice-ish": MODERN }, "options": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": MODERN, "thermal-receipt": "doc_temp_nope" }, "options": {} })),
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
    for name in ["doc_temp_nope", GONE, ANALYTICS] {
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

#[tokio::test]
async fn an_admin_saves_option_values_and_the_listing_reports_them() {
    let app = common::spawn_app_sqlite().await;
    let admin = token(&app, Role::Admin);

    let (status, body) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({
            "selections": { "a4-invoice": CLIENT },
            "options": { CLIENT: { "accent": "red" } }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = kind(&body, "a4-invoice");
    assert_eq!(invoice["selected"], CLIENT);
    assert_eq!(invoice["effective"], CLIENT);
    // What was saved, plus the design's own default for what was not.
    assert_eq!(
        invoice["optionValues"][CLIENT],
        json!({ "showSign": true, "accent": "red" })
    );

    // A later update changes only the values it names.
    let (_, body) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": {}, "options": { CLIENT: { "showSign": false } } })),
    )
    .await;
    assert_eq!(
        kind(&body, "a4-invoice")["optionValues"][CLIENT],
        json!({ "showSign": false, "accent": "red" })
    );
}

#[tokio::test]
async fn invalid_option_values_are_rejected_and_change_nothing() {
    let app = common::spawn_app_sqlite().await;
    let admin = token(&app, Role::Admin);
    for (label, options) in [
        ("not a choice", json!({ CLIENT: { "accent": "purple" } })),
        ("wrong type", json!({ CLIENT: { "showSign": "yes" } })),
        ("unknown option", json!({ CLIENT: { "nope": 1 } })),
        ("unknown design", json!({ GONE: { "accent": "red" } })),
        (
            "design without options",
            json!({ CLASSIC: { "accent": "red" } }),
        ),
    ] {
        let (status, body) = call_json(
            &app.router,
            "PUT",
            SETTINGS,
            &admin,
            Some(json!({ "selections": { "a4-invoice": CLIENT }, "options": options })),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{label}: {body}");
    }
    let (_, after) = call_json(&app.router, "GET", SETTINGS, &admin, None).await;
    assert_eq!(kind(&after, "a4-invoice")["selected"], Value::Null);
}

#[tokio::test]
async fn a_stored_choice_that_is_no_longer_offered_is_reported_not_replaced() {
    let app = common::spawn_app_sqlite().await;
    // Written as it would have been stored while the design still existed.
    let simplebash_pos_backend::clients::db::Db::Sqlite(pool) = &app.db_handle else {
        panic!("sqlite app");
    };
    sqlx::query(
        "INSERT INTO document_template_settings (key, selections, options, version, created_at, updated_at) \
         VALUES ('main', ?1, '{}', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(json!({ "a4-invoice": GONE }).to_string())
    .execute(pool)
    .await
    .unwrap();

    let (status, body) = call_json(
        &app.router,
        "GET",
        SETTINGS,
        &token(&app, Role::Staff),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = kind(&body, "a4-invoice");
    assert_eq!(invoice["selected"], GONE);
    assert_eq!(
        invoice["effective"],
        Value::Null,
        "nothing is in force: the choice must be redone"
    );
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
    // `query` is extra `&…` parameters; `design=current` unless it names one.
    let design = if query.contains("design=") {
        ""
    } else {
        "&design=current"
    };
    let (status, bytes) = call(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}/documents/{document}?x=1{design}{query}"),
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
        Some(json!({ "selections": { "a4-invoice": MODERN }, "options": {} })),
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
        Some(json!({ "selections": { "a4-invoice": null }, "options": {} })),
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
        &format!("&template={MODERN}"),
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
        &format!("&paperWidthMm=58&template={COMPACT_RECEIPT}"),
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

    // A design that does not exist, was revoked, or belongs to another kind of
    // document is a clear error rather than a silent switch.
    for bad in ["doc_temp_nope", GONE, COMPACT_RECEIPT, "..%2Fetc"] {
        let (status, body) = print(
            &app,
            &cashier,
            &invoice,
            "a4-invoice",
            &format!("&template={bad}"),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert!(body.contains("TEMPLATE_UNAVAILABLE"), "{bad}: {body}");
    }

    app.db.drop().await.ok();
}

#[tokio::test]
async fn a_design_that_is_no_longer_offered_blocks_printing_until_the_shop_chooses_again() {
    let app = common::spawn_app_isolated().await;
    let cashier = token(&app, Role::Staff);
    let admin = token(&app, Role::Admin);
    let invoice = make_sale(&app, &cashier).await;

    // A choice made while the design was still offered; the settings API
    // would refuse it now, so write it as it would have been stored.
    let now = mongodb::bson::DateTime::now();
    app.db
        .collection::<Document>("document_template_settings")
        .insert_one(doc! {
            "key": "main",
            "selections": { "a4-invoice": GONE },
            "options": {},
            "version": 1,
            "created_at": now,
            "updated_at": now,
        })
        .await
        .unwrap();

    // No silent substitution: the print is refused with a clear code...
    let (status, body) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("TEMPLATE_UNAVAILABLE"), "{body}");
    // ...other kinds are unaffected...
    let (status, _) = print(&app, &cashier, &invoice, "thermal-receipt", "").await;
    assert_eq!(status, StatusCode::OK);
    // ...and Settings reports it honestly: nothing in force.
    let (_, settings) = call_json(&app.router, "GET", SETTINGS, &cashier, None).await;
    assert_eq!(kind(&settings, "a4-invoice")["selected"], GONE);
    assert_eq!(kind(&settings, "a4-invoice")["effective"], Value::Null);

    // Choosing again (here the standard design) restores printing.
    let (status, _) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": null }, "options": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.contains("tpl_a4_invoice "), "{pdf}");

    app.db.drop().await.ok();
}

#[tokio::test]
async fn the_document_endpoints_require_the_design_parameter() {
    let app = common::spawn_app_isolated().await;
    let cashier = token(&app, Role::Staff);
    let invoice = make_sale(&app, &cashier).await;
    let (status, _) = call(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice}/documents/a4-invoice"),
        Some(&cashier),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.db.drop().await.ok();
}

async fn design_status(app: &common::TestApp, cashier: &str, invoice: &str, query: &str) -> Value {
    let (status, body) = call_json(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice}/documents/a4-invoice/design{query}"),
        cashier,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

#[tokio::test]
async fn reprinting_can_serve_the_original_design_or_the_current_one() {
    let app = common::spawn_app_isolated().await;
    let cashier = token(&app, Role::Staff);
    let admin = token(&app, Role::Admin);
    let invoice = make_sale(&app, &cashier).await;

    // Nothing was printed yet: no stored design, nothing differs, no original.
    let status = design_status(&app, &cashier, &invoice, "").await;
    assert_eq!(status["stored"], Value::Null);
    assert_eq!(status["current"]["templateRef"], CLASSIC);
    assert_eq!(status["differs"], false);
    let (code, body) = print(&app, &cashier, &invoice, "a4-invoice", "&design=original").await;
    assert_eq!(code, StatusCode::NOT_FOUND, "{body}");

    // Print with the standard design; the row records which design it was.
    let (_, first) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert!(first.contains("tpl_a4_invoice "), "{first}");
    let status = design_status(&app, &cashier, &invoice, "").await;
    assert_eq!(status["stored"]["templateRef"], CLASSIC);
    assert_eq!(status["stored"]["title"], "A4 Invoice");
    assert_eq!(status["stored"]["version"], 1);
    assert_eq!(status["differs"], false);

    // The shop moves to Modern: the stored print is now another design.
    call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": MODERN }, "options": {} })),
    )
    .await;
    let status = design_status(&app, &cashier, &invoice, "").await;
    assert_eq!(status["stored"]["templateRef"], CLASSIC);
    assert_eq!(status["current"]["templateRef"], MODERN);
    assert_eq!(status["differs"], true);

    // Original: the stored PDF exactly, and nothing new is rendered. Current: Modern.
    let (code, original) = print(&app, &cashier, &invoice, "a4-invoice", "&design=original").await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(original, first);
    assert_eq!(rows(&app, &invoice).await.len(), 1);
    let (_, current) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert!(current.contains("tpl_a4_invoice_modern"), "{current}");

    // Original still answers while the choice is unavailable (the stored file is
    // all it needs), and serves the newest stored print of that variant.
    let (_, original) = print(&app, &cashier, &invoice, "a4-invoice", "&design=original").await;
    assert!(original.contains("tpl_a4_invoice_modern"), "{original}");

    // Thermal receipts keep their paper widths apart.
    print(
        &app,
        &cashier,
        &invoice,
        "thermal-receipt",
        "&paperWidthMm=58",
    )
    .await;
    let (code, _) = print(
        &app,
        &cashier,
        &invoice,
        "thermal-receipt",
        "&paperWidthMm=80&design=original",
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND, "no 80 mm receipt was printed");

    app.db.drop().await.ok();
}

#[tokio::test]
async fn a_custom_design_prints_with_its_options_and_each_value_set_is_cached_apart() {
    let app = common::spawn_app_isolated().await;
    let cashier = token(&app, Role::Staff);
    let admin = token(&app, Role::Admin);
    let invoice = make_sale(&app, &cashier).await;

    call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": { "a4-invoice": CLIENT }, "options": {} })),
    )
    .await;
    // The design's declared defaults travel in the payload.
    let (status, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert_eq!(status, StatusCode::OK, "{pdf}");
    assert!(pdf.contains("tpl_client_invoice"), "{pdf}");
    assert!(
        pdf.contains(r#""options":{"showSign":true,"accent":"blue"}"#),
        "{pdf}"
    );

    // Changing a setting renders again instead of serving the old PDF.
    call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": {}, "options": { CLIENT: { "accent": "red" } } })),
    )
    .await;
    let (_, pdf) = print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert!(pdf.contains(r#""accent":"red""#), "{pdf}");
    let stored = rows(&app, &invoice).await;
    assert_eq!(stored.len(), 2, "one PDF per option set: {stored:?}");
    assert!(
        stored
            .iter()
            .all(|r| r.get_str("template_ref").ok() == Some(CLIENT_NAME)
                && r.get_i64("template_version").ok() == Some(2))
    );

    // Printing with the first values again is a cache hit.
    call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({ "selections": {}, "options": { CLIENT: { "accent": "blue" } } })),
    )
    .await;
    print(&app, &cashier, &invoice, "a4-invoice", "").await;
    assert_eq!(rows(&app, &invoice).await.len(), 2);

    app.db.drop().await.ok();
}

#[tokio::test]
async fn resetting_a_designs_selections_clears_the_choice_and_its_option_values() {
    // The operator console's calls, against a multi-tenant deployment.
    let secret = "doc-selections-test-secret".to_string();
    let app = common::spawn_app_multi_tenant_with_secret(Some(&secret)).await;
    let admin = common::mint_token_for_tenant(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
        Some("tnt_shop_a"),
    );
    // The shop must exist in the directory for the internal routes.
    let created = simplebash_pos_backend::modules::tenants::service::create_tenant_with_key(
        &app.db_handle,
        "tnt_shop_a".to_string(),
        "shop-a",
        "Shop A",
    )
    .await;
    assert!(created.is_ok(), "{created:?}");

    let (status, _) = call_json(
        &app.router,
        "PUT",
        SETTINGS,
        &admin,
        Some(json!({
            "selections": { "a4-invoice": CLIENT },
            "options": { CLIENT: { "accent": "red" } }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let internal = |method: &str, uri: &str, body: Option<Value>, secret: &str| {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .header("x-provision-secret", secret)
            .body(match body {
                Some(value) => Body::from(value.to_string()),
                None => Body::empty(),
            })
            .unwrap();
        let router = app.router.clone();
        async move {
            let response = router.oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };
    let list = "/api/internal/tenants/tnt_shop_a/document-selections";

    let (status, body) = internal("GET", list, None, &secret).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["selections"],
        json!([{ "kind": "a4-invoice", "family": CLIENT }])
    );
    assert_eq!(
        internal("GET", list, None, "wrong-secret-wrong").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        internal(
            "GET",
            "/api/internal/tenants/tnt_nobody/document-selections",
            None,
            &secret
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    let reset = format!("{list}/reset");
    assert_eq!(
        internal("POST", &reset, Some(json!({ "families": [] })), &secret)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let (status, body) = internal(
        "POST",
        &reset,
        Some(json!({ "families": [CLIENT, CLASSIC] })),
        &secret,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["reset"],
        json!([{ "kind": "a4-invoice", "family": CLIENT }])
    );

    let (_, settings) = call_json(&app.router, "GET", SETTINGS, &admin, None).await;
    let invoice = kind(&settings, "a4-invoice");
    assert_eq!(invoice["selected"], Value::Null);
    assert_eq!(
        invoice["optionValues"][CLIENT],
        json!({ "showSign": true, "accent": "blue" }),
        "the saved values went with the choice"
    );
    // Idempotent.
    let (status, body) = internal(
        "POST",
        &reset,
        Some(json!({ "families": [CLIENT] })),
        &secret,
    )
    .await;
    assert_eq!(
        (status, &body["data"]["reset"]),
        (StatusCode::OK, &json!([]))
    );

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
        Some(json!({ "selections": { "a4-invoice": MODERN }, "options": {} })),
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
