//! Quotation access tests.

use axum::http::StatusCode;

use serde_json::{Value, json};

use simplebash_pos_backend::core::constants::{permissions as perm, roles};
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{call, create, date_in, request, setup, token, valid_body};

// ============================================================================
// Access control
// ============================================================================

#[tokio::test]
async fn only_admin_and_manager_can_use_quotations() {
    let ctx = setup().await;
    let admin = token(&ctx.config, Role::Admin);
    let manager = token(&ctx.config, Role::Manager);
    let staff = token(&ctx.config, Role::Staff);
    // A token that has a role but none of the permissions.
    let bare = common::mint_token(&ctx.config, Some(Role::Manager), &[]);

    // Admin and Manager can create.
    for tok in [&admin, &manager] {
        let (status, body) = create(&ctx.router, tok, valid_body()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (_, created) = create(&ctx.router, &admin, valid_body()).await;
    let key = created["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // Staff and permission-less tokens are refused on every route.
    for tok in [&staff, &bare] {
        for (method, uri, body) in [
            ("GET", "/api/quotations".to_string(), None),
            ("POST", "/api/quotations".to_string(), Some(valid_body())),
            ("GET", format!("/api/quotations/{key}"), None),
            ("POST", format!("/api/quotations/{key}/cancel"), None),
            (
                "GET",
                format!("/api/quotations/{key}/document?design=current"),
                None,
            ),
        ] {
            let (status, _, bytes) = request(&ctx.router, method, &uri, body, Some(tok)).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
            let json: Value = serde_json::from_slice(&bytes).expect("the text is valid JSON");
            assert_eq!(json["code"], "PERMISSION_DENIED");
        }
    }

    // And a missing token is a 401, not a 403.
    let (status, _, _) = request(&ctx.router, "GET", "/api/quotations", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The permission itself is held by exactly Admin and Manager.
    assert!(roles::default_permissions(Role::Admin).contains(&perm::QUOTATIONS_MANAGE));
    assert!(roles::default_permissions(Role::Manager).contains(&perm::QUOTATIONS_MANAGE));
    assert!(!roles::default_permissions(Role::Staff).contains(&perm::QUOTATIONS_MANAGE));

    // Staff really saw nothing created on their behalf.
    let (_, list) = call(&ctx.router, "GET", "/api/quotations", None, &admin).await;
    assert_eq!(list["data"]["total"], 3);
}

// ============================================================================
// Create: totals, numbering, validation
// ============================================================================

#[tokio::test]
async fn totals_are_computed_by_the_server_and_numbers_increment() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Manager);

    // Client-supplied totals must be ignored.
    let mut body = valid_body();
    body["subtotalCents"] = json!(1);
    body["totalCents"] = json!(1);
    let (status, first) = create(&ctx.router, &tok, body).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let q = &first["data"];

    // Lines: 10000*2-500 = 19500 and 2500 -> subtotal 22000; 10% -> 2200.
    assert_eq!(q["items"][0]["totalCents"], 19500);
    assert_eq!(q["items"][1]["totalCents"], 2500);
    assert_eq!(q["subtotalCents"], 22000);
    assert_eq!(q["discountType"], "percentage");
    assert_eq!(q["discountCents"], 2200);
    assert_eq!(q["totalCents"], 19800);
    assert_eq!(q["status"], "active");
    assert_eq!(q["isExpired"], false);
    assert_eq!(q["customerName"], "Nimal Perera");
    assert_eq!(q["preparedBy"], "Kamal");
    assert!(
        q["key"]
            .as_str()
            .expect("the value is a string")
            .starts_with("quo_")
    );
    // The frozen shop profile is not leaked through the API.
    assert!(q.get("shopProfileSnapshot").is_none());

    assert_eq!(q["quotationNumber"], "QUO-0001");
    let (_, second) = create(&ctx.router, &tok, valid_body()).await;
    assert_eq!(second["data"]["quotationNumber"], "QUO-0002");

    // A fixed order discount, and no discount at all.
    let mut fixed = valid_body();
    fixed["pricingAdjustments"] = json!({ "discountType": "fixed", "discountValue": 1000.0 });
    let (_, fixed_res) = create(&ctx.router, &tok, fixed).await;
    assert_eq!(fixed_res["data"]["discountCents"], 1000);
    assert_eq!(fixed_res["data"]["totalCents"], 21000);

    let mut none = valid_body();
    none.as_object_mut()
        .expect("the value is an object")
        .remove("pricingAdjustments");
    let (_, none_res) = create(&ctx.router, &tok, none).await;
    assert_eq!(none_res["data"]["discountCents"], 0);
    assert_eq!(none_res["data"]["totalCents"], 22000);
}

#[tokio::test]
async fn invalid_requests_are_rejected_and_do_not_burn_a_number() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);

    // (label, how to break an otherwise valid request)
    type Case = (&'static str, Box<dyn Fn(&mut Value)>);
    let cases: Vec<Case> = vec![
        ("no items", Box::new(|b| b["items"] = json!([]))),
        (
            "zero quantity",
            Box::new(|b| b["items"][0]["quantity"] = json!(0)),
        ),
        (
            "negative price",
            Box::new(|b| b["items"][0]["unitPriceCents"] = json!(-1)),
        ),
        (
            "missing price",
            Box::new(|b| {
                b["items"][0]
                    .as_object_mut()
                    .expect("the value is an object")
                    .remove("unitPriceCents");
            }),
        ),
        (
            "blank item name",
            Box::new(|b| b["items"][0]["name"] = json!("   ")),
        ),
        (
            "line discount above price",
            Box::new(|b| b["items"][0]["discountCents"] = json!(20001)),
        ),
        (
            "negative line discount",
            Box::new(|b| b["items"][0]["discountCents"] = json!(-5)),
        ),
        (
            "past valid-until",
            Box::new(|b| b["validUntil"] = json!(date_in(-1))),
        ),
        (
            "malformed valid-until",
            Box::new(|b| b["validUntil"] = json!("31/12/2030")),
        ),
        (
            "percentage over 100",
            Box::new(|b| {
                b["pricingAdjustments"] =
                    json!({ "discountType": "percentage", "discountValue": 101.0 })
            }),
        ),
        (
            "unknown discount type",
            Box::new(|b| {
                b["pricingAdjustments"] = json!({ "discountType": "bogo", "discountValue": 1.0 })
            }),
        ),
        (
            "negative order discount",
            Box::new(|b| {
                b["pricingAdjustments"] = json!({ "discountType": "fixed", "discountValue": -1.0 })
            }),
        ),
        (
            "no customer name",
            Box::new(|b| b["customer"] = json!({ "customerPhone": "0771234567" })),
        ),
        (
            "overlong notes",
            Box::new(|b| b["notes"] = json!("x".repeat(2001))),
        ),
    ];
    for (label, mutate) in cases {
        let mut body = valid_body();
        mutate(&mut body);
        let (status, res) = create(&ctx.router, &tok, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{label}: {res}");
    }

    // Unknown references are 404s, also before any number is reserved.
    let mut bad_product = valid_body();
    bad_product["items"][0] = json!({ "productKey": "prod_nope", "quantity": 1 });
    assert_eq!(
        create(&ctx.router, &tok, bad_product).await.0,
        StatusCode::NOT_FOUND
    );
    let mut bad_customer = valid_body();
    bad_customer["customer"] = json!({ "customerKey": "cust_nope" });
    assert_eq!(
        create(&ctx.router, &tok, bad_customer).await.0,
        StatusCode::NOT_FOUND
    );

    // Fifteen rejected requests later, the first real quotation is still #1.
    let (status, ok) = create(&ctx.router, &tok, valid_body()).await;
    assert_eq!(status, StatusCode::OK, "{ok}");
    assert_eq!(ok["data"]["quotationNumber"], "QUO-0001");

    // Valid-until of today is allowed (the last day the offer holds).
    let mut today = valid_body();
    today["validUntil"] = json!(date_in(0));
    assert_eq!(create(&ctx.router, &tok, today).await.0, StatusCode::OK);
}

#[tokio::test]
async fn catalog_lines_use_the_catalog_and_do_not_touch_stock_or_invoices() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);

    let (_, cat) = call(
        &ctx.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Phones", "icon": "devices", "color": "blue" })),
        &tok,
    )
    .await;
    let category_key = cat["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (_, sub) = call(
        &ctx.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Smart" })),
        &tok,
    )
    .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (status, product) = call(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Galaxy A15", "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } }, "minStockThreshold": 2,
            "openingStock": { "quantity": 10, "unitCostCents": 50000, "sellingPriceCents": 65000 }
        })),
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let product_key = product["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let product_id = product["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let sku = product["data"]["sku"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (_, customer) = call(
        &ctx.router,
        "POST",
        "/api/customers",
        Some(
            json!({ "name": "Registered Rita", "primaryPhone": "0712223334",
                     "address": "5 Main St" }),
        ),
        &tok,
    )
    .await;
    let customer_key = customer["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (_, invoices_before) = call(&ctx.router, "GET", "/api/billing/invoices", None, &tok).await;

    // A product line must quote one of the product's current prices.
    for (line, code) in [
        (
            json!({ "productKey": product_key, "quantity": 1 }),
            "VALIDATION_ERROR",
        ),
        (
            json!({ "productKey": product_key, "sellingPriceCents": 70000, "quantity": 1 }),
            "PRICE_TIER_UNAVAILABLE",
        ),
    ] {
        let (status, res) = create(
            &ctx.router,
            &tok,
            json!({ "customer": { "customerName": "Walk-in" }, "items": [line], "validUntil": date_in(7) }),
        )
        .await;
        assert!(status.is_client_error(), "{res}");
        assert_eq!(res["code"], code, "{res}");
    }

    // The client lies about name and unit price; the catalog and the picked
    // price win. The registered customer's record wins over the typed name.
    let (status, res) = create(
        &ctx.router,
        &tok,
        json!({
            "customer": { "customerKey": customer_key, "customerName": "Somebody Else" },
            "items": [{ "productKey": product_key, "name": "Fake", "unitPriceCents": 1, "sellingPriceCents": 65000, "quantity": 3 }],
            "validUntil": date_in(7)
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let q = &res["data"];
    assert_eq!(q["items"][0]["name"], "Galaxy A15");
    assert_eq!(q["items"][0]["sku"], sku);
    assert_eq!(q["items"][0]["unitPriceCents"], 65000);
    assert_eq!(q["totalCents"], 195000);
    assert_eq!(q["customerName"], "Registered Rita");
    assert_eq!(q["customerPhone"], "0712223334");
    assert_eq!(q["customerAddress"], "5 Main St");
    assert_eq!(q["customerKey"], customer_key);

    // Stock is untouched and no invoice appeared.
    let (_, after) = call(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &tok,
    )
    .await;
    assert_eq!(after["data"]["stockQuantity"], 10);
    let (_, invoices_after) = call(&ctx.router, "GET", "/api/billing/invoices", None, &tok).await;
    assert_eq!(
        invoices_before["data"]["total"],
        invoices_after["data"]["total"]
    );
}

// ============================================================================
// Read: get, list filters, expiry
// ============================================================================

#[tokio::test]
async fn list_search_status_filters_and_pagination() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Manager);

    let mut for_amal = valid_body();
    for_amal["customer"]["customerName"] = json!("Amal Silva");
    let (_, amal) = create(&ctx.router, &tok, for_amal).await;
    let (_, nimal) = create(&ctx.router, &tok, valid_body()).await;
    let (_, expiring) = create(&ctx.router, &tok, valid_body()).await;

    // Cancel one; push another into the past (the API refuses to create one).
    let cancel_key = amal["data"]["key"].as_str().expect("the value is a string");
    call(
        &ctx.router,
        "POST",
        &format!("/api/quotations/{cancel_key}/cancel"),
        None,
        &tok,
    )
    .await;
    let pool = sqlx::SqlitePool::connect(&ctx.db_url)
        .await
        .expect("the awaited step succeeds");
    sqlx::query("UPDATE quotations SET valid_until = '2000-01-01' WHERE key = ?")
        .bind(
            expiring["data"]["key"]
                .as_str()
                .expect("the value is a string"),
        )
        .execute(&pool)
        .await
        .expect("the awaited step succeeds");

    let list = |query: &'static str| {
        let router = ctx.router.clone();
        let tok = tok.clone();
        async move {
            call(
                &router,
                "GET",
                &format!("/api/quotations{query}"),
                None,
                &tok,
            )
            .await
        }
    };

    let (status, all) = list("").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["data"]["total"], 3);
    // Newest first.
    assert_eq!(all["data"]["quotations"][0]["key"], expiring["data"]["key"]);

    let (_, active) = list("?status=active").await;
    assert_eq!(active["data"]["total"], 1);
    assert_eq!(active["data"]["quotations"][0]["key"], nimal["data"]["key"]);

    let (_, expired) = list("?status=expired").await;
    assert_eq!(expired["data"]["total"], 1);
    assert_eq!(expired["data"]["quotations"][0]["isExpired"], true);
    assert_eq!(expired["data"]["quotations"][0]["status"], "active");

    let (_, cancelled) = list("?status=cancelled").await;
    assert_eq!(cancelled["data"]["total"], 1);
    assert_eq!(
        cancelled["data"]["quotations"][0]["customerName"],
        "Amal Silva"
    );
    // A cancelled quotation is never reported as expired.
    assert_eq!(cancelled["data"]["quotations"][0]["isExpired"], false);

    // Fulfilled status is a valid filter and returns 200 (not 400).
    let (status_fulfilled, fulfilled) = list("?status=fulfilled").await;
    assert_eq!(status_fulfilled, StatusCode::OK);
    assert_eq!(fulfilled["data"]["total"], 0);

    // Search by customer name, by number and by phone.
    assert_eq!(list("?search=amal").await.1["data"]["total"], 1);
    assert_eq!(list("?search=QUO-0002").await.1["data"]["total"], 1);
    // All three share the default phone number.
    assert_eq!(list("?search=0771234567").await.1["data"]["total"], 3);
    assert_eq!(list("?search=nobody-here").await.1["data"]["total"], 0);

    // Unknown status is a 400, not an empty list.
    assert_eq!(list("?status=bogus").await.0, StatusCode::BAD_REQUEST);

    // Pagination.
    let (_, page) = list("?limit=2&page=2").await;
    assert_eq!(page["data"]["total"], 3);
    assert_eq!(page["data"]["totalPages"], 2);
    assert_eq!(
        page["data"]["quotations"]
            .as_array()
            .expect("the value is an array")
            .len(),
        1
    );
}

#[tokio::test]
async fn get_by_key_and_by_id_and_missing_is_404() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);
    let (_, created) = create(&ctx.router, &tok, valid_body()).await;
    let key = created["data"]["key"]
        .as_str()
        .expect("the value is a string");
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string");

    for handle in [key, id] {
        let (status, res) = call(
            &ctx.router,
            "GET",
            &format!("/api/quotations/{handle}"),
            None,
            &tok,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        assert_eq!(res["data"]["quotationNumber"], "QUO-0001");
    }

    let (status, res) = call(
        &ctx.router,
        "GET",
        "/api/quotations/quo_missing",
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(res["code"], "QUOTATION_NOT_FOUND");
}

// ============================================================================
// Cancel + PDF
// ============================================================================

#[tokio::test]
async fn cancel_once_then_conflict() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Manager);
    let (_, created) = create(&ctx.router, &tok, valid_body()).await;
    let key = created["data"]["key"]
        .as_str()
        .expect("the value is a string");

    let (status, res) = call(
        &ctx.router,
        "POST",
        &format!("/api/quotations/{key}/cancel"),
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "cancelled");

    let (status, res) = call(
        &ctx.router,
        "POST",
        &format!("/api/quotations/{key}/cancel"),
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(res["code"], "QUOTATION_ALREADY_CANCELLED");

    let (status, _) = call(
        &ctx.router,
        "POST",
        "/api/quotations/quo_missing/cancel",
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
