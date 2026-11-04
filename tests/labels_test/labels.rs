//! Labels tests.

use axum::http::StatusCode;

use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{
    PRICE_TAG, Shop, barcode, design, rendered_payload, scenario, send, send_raw,
};

/// Prints, downloads and failed prints are recorded with what, how and who; the
/// coverage view tells never labelled, labels needed for new stock, and labelled.
async fn jobs_scenario(app: &common::TestApp) {
    let shop = Shop::new(app).await;
    let cable = shop
        .product("Cable", json!({ "autoGenerateBarcode": true }))
        .await;
    let cable_key = cable["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    shop.purchase(&cable_key, 5, 150_000, None).await;
    let charger = shop
        .product("Charger", json!({ "autoGenerateBarcode": true }))
        .await;
    let charger_key = charger["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    shop.purchase(&charger_key, 2, 90_000, None).await;
    let empty = shop
        .product("Empty shelf", json!({ "autoGenerateBarcode": true }))
        .await;
    let phone = shop.product("Phone", json!({ "isSerialized": true })).await;
    let phone_key = phone["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let imei = uuid::Uuid::new_v4().as_u128() % 100_000_000;
    let serials: Vec<String> = (1..=2).map(|i| format!("356938{imei:08}{i}")).collect();
    shop.purchase(&phone_key, 2, 80_000, Some(serials.clone()))
        .await;

    let search = format!("/api/labels/coverage?search={}", shop.tag);
    let row_of = |coverage: &Value, key: &str| -> Value {
        coverage["items"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .find(|row| row["product"]["key"] == key)
            .cloned()
            .unwrap_or_else(|| panic!("{key} missing from {coverage}"))
    };

    // ---- before any print: in-stock items were never labelled ------------------
    let coverage = shop.get(&search).await;
    assert_eq!(coverage["counts"]["never"], 3, "{coverage}");
    assert_eq!(coverage["counts"]["needed"], 0);
    assert_eq!(coverage["counts"]["labelled"], 0);
    assert_eq!(row_of(&coverage, &cable_key)["status"], "never");
    assert!(row_of(&coverage, &cable_key)["lastLabelledAt"].is_null());
    // Nothing in stock and never labelled: nothing to label, listed last.
    let empty_row = row_of(
        &coverage,
        empty["key"].as_str().expect("the value is a string"),
    );
    assert!(empty_row["status"].is_null());
    assert_eq!(
        coverage["items"]
            .as_array()
            .expect("the value is an array")
            .last()
            .expect("the list is not empty")["product"]["key"],
        empty["key"]
    );

    // ---- searching narrows the list, the same on SQLite and MongoDB ------------
    let charger_search = format!("/api/labels/coverage?search=Charger%20{}", shop.tag);
    let found = shop.get(&charger_search).await;
    assert_eq!(found["pagination"]["total"], 1, "{found}");
    assert_eq!(found["items"][0]["product"]["key"], charger_key.as_str());
    let (status, products) = send(
        &shop.router,
        "GET",
        &format!("/api/inventory/products?search=charger%20{}", shop.tag),
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{products}");
    assert_eq!(products["data"]["pagination"]["total"], 1, "{products}");
    // A wildcard typed by the person is just a character.
    let none = shop
        .get(&format!("/api/labels/coverage?search=Cab%25{}", shop.tag))
        .await;
    assert_eq!(none["pagination"]["total"], 0, "{none}");

    // ---- a print to the system print window --------------------------------------
    let item = |key: &str, copies: i64, price: Value| json!({ "kind": "product", "productKey": key, "copies": copies, "priceCents": price });
    let (status, key, pdf) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": [item(&cable_key, 3, json!(150_000))],
            "action": "print", "method": "system", "printerName": null, "dpi": 203,
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    assert_eq!(rendered_payload(&pdf)["labels"][0]["copies"], 3);
    let print_key = key.expect("the job key comes back in a header");
    assert!(print_key.starts_with("lpj_"));
    let job = shop.get(&format!("/api/labels/jobs/{print_key}")).await;
    assert_eq!(job["outcome"], "rendered");
    assert_eq!(job["action"], "print");
    assert_eq!(job["method"], "system");
    assert_eq!(job["dpi"], 203);
    assert!(job["printerName"].is_null());
    assert_eq!(job["templateKey"], PRICE_TAG);
    assert_eq!(job["templateName"], "Price tag 50 x 30 mm");
    assert_eq!(job["totalLabels"], 3);
    assert!(
        !job["printedByName"]
            .as_str()
            .expect("the value is a string")
            .is_empty()
    );
    let printed = &job["items"][0];
    assert_eq!(printed["productKey"], cable_key.as_str());
    assert_eq!(
        printed["productName"],
        format!("Cable {}", shop.tag).as_str()
    );
    assert_eq!(printed["priceCents"], 150_000);
    assert_eq!(printed["copies"], 3);
    assert!(printed["serialNumber"].is_null());

    // The computer reports back once; a second report or a bad one is refused.
    let uri = format!("/api/labels/jobs/{print_key}");
    for (body, expected) in [
        (
            json!({ "outcome": "rendered", "error": null }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "outcome": "sent", "error": "why" }),
            StatusCode::BAD_REQUEST,
        ),
        (json!({ "outcome": "sent", "error": null }), StatusCode::OK),
        (
            json!({ "outcome": "failed", "error": "jam" }),
            StatusCode::CONFLICT,
        ),
    ] {
        let (status, response) = send(
            &shop.router,
            "PATCH",
            &uri,
            Some(body.clone()),
            &shop.manager,
        )
        .await;
        assert_eq!(status, expected, "{body}: {response}");
    }
    assert_eq!(shop.get(&uri).await["outcome"], "sent");
    let (status, body) = send(
        &shop.router,
        "PATCH",
        "/api/labels/jobs/lpj_missing",
        Some(json!({ "outcome": "sent", "error": null })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "LABEL_JOB_NOT_FOUND");

    // ---- a failed direct print does not count as labelled ------------------------
    let (status, key, _) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": [item(&charger_key, 2, json!(90_000))],
            "action": "print", "method": "direct", "printerName": " Zebra ZD220 ", "dpi": 300,
        }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let failed_uri = format!("/api/labels/jobs/{}", key.expect("the step succeeds"));
    let (status, failed) = send(
        &shop.router,
        "PATCH",
        &failed_uri,
        Some(json!({ "outcome": "failed", "error": "Printer is offline" })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert_eq!(failed["data"]["outcome"], "failed");
    assert_eq!(failed["data"]["error"], "Printer is offline");
    assert_eq!(failed["data"]["printerName"], "Zebra ZD220");

    // ---- serial labels: one group per unit, recorded with the serial number ----
    let (status, _, _) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": serials.iter().map(|n| json!({ "kind": "serial", "serialNumber": n, "copies": 2 })).collect::<Vec<_>>(),
            "action": "download", "method": null, "printerName": null, "dpi": null,
        }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let downloads = shop
        .get(&format!(
            "/api/labels/jobs?action=download&productKey={phone_key}"
        ))
        .await;
    assert_eq!(downloads["pagination"]["total"], 1, "{downloads}");
    let download = &downloads["items"][0];
    assert_eq!(download["outcome"], "sent");
    assert!(download["method"].is_null() && download["dpi"].is_null());
    assert_eq!(download["totalLabels"], 4);
    assert_eq!(download["items"][1]["serialNumber"], serials[1].as_str());
    assert_eq!(download["items"][1]["productKey"], phone_key.as_str());
    // A download was handed over at once: there is nothing to report.
    let (status, body) = send(
        &shop.router,
        "PATCH",
        &format!(
            "/api/labels/jobs/{}",
            download["key"].as_str().expect("the value is a string")
        ),
        Some(json!({ "outcome": "sent", "error": null })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "LABEL_JOB_ALREADY_REPORTED");

    // ---- incomplete print methods and unprintable items are not recorded --------
    for (body, why) in [
        (
            json!({ "action": "download", "method": "system", "printerName": null, "dpi": null }),
            "download with a method",
        ),
        (
            json!({ "action": "print", "method": "system", "printerName": null, "dpi": null }),
            "print without dpi",
        ),
        (
            json!({ "action": "print", "method": "direct", "printerName": null, "dpi": 203 }),
            "direct print without a printer",
        ),
        (
            json!({ "action": "print", "method": null, "printerName": null, "dpi": 203 }),
            "print without a method",
        ),
    ] {
        let mut request = body;
        request["templateKey"] = json!(PRICE_TAG);
        request["items"] = json!([item(&cable_key, 1, json!(150_000))]);
        let (status, key, _) = shop.print_job(request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}");
        assert!(key.is_none());
    }
    let (status, key, _) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": [item(&cable_key, 1, Value::Null)],
            "action": "download", "method": null, "printerName": null, "dpi": null,
        }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(key.is_none());

    // ---- history listing ------------------------------------------------------------
    let all = shop.get("/api/labels/jobs?limit=100").await;
    let mine: Vec<&Value> = all["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .filter(|job| {
            job["items"]
                .as_array()
                .expect("the value is an array")
                .iter()
                .any(|i| {
                    i["productName"]
                        .as_str()
                        .expect("the value is a string")
                        .ends_with(&shop.tag)
                })
        })
        .collect();
    assert_eq!(mine.len(), 3, "{all}");
    // Newest first: the download, then the failed print, then the first print.
    assert_eq!(mine[0]["action"], "download");
    assert_eq!(mine[2]["key"], print_key.as_str());
    let page = shop.get("/api/labels/jobs?limit=1&page=2").await;
    assert_eq!(
        page["items"]
            .as_array()
            .expect("the value is an array")
            .len(),
        1
    );
    assert_eq!(page["pagination"]["limit"], 1);
    let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let none = shop
        .get(&format!(
            "/api/labels/jobs?from={}",
            future.replace('+', "%2B")
        ))
        .await;
    assert_eq!(none["pagination"]["total"], 0);
    let (status, body) = send(
        &shop.router,
        "GET",
        "/api/labels/jobs?from=yesterday",
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // ---- coverage after printing -------------------------------------------------------
    let coverage = shop.get(&search).await;
    let cable_row = row_of(&coverage, &cable_key);
    assert_eq!(cable_row["status"], "labelled", "{coverage}");
    assert_eq!(cable_row["lastLabelledCopies"], 3);
    assert_eq!(cable_row["receivedSinceLabel"], 0);
    assert_eq!(row_of(&coverage, &charger_key)["status"], "never");
    let phone_row = row_of(&coverage, &phone_key);
    assert_eq!(phone_row["status"], "labelled");
    assert_eq!(phone_row["lastLabelledCopies"], 4);

    // New stock after the last label means its labels are needed.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    shop.purchase(&cable_key, 4, 160_000, None).await;
    let coverage = shop.get(&search).await;
    let cable_row = row_of(&coverage, &cable_key);
    assert_eq!(cable_row["status"], "needed", "{coverage}");
    assert_eq!(cable_row["receivedSinceLabel"], 4);
    assert_eq!(coverage["counts"]["never"], 1);
    assert_eq!(coverage["counts"]["needed"], 1);
    assert_eq!(coverage["counts"]["labelled"], 1);
    // Never labelled first, then labels needed, then labelled.
    let order: Vec<&str> = coverage["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|row| {
            row["product"]["key"]
                .as_str()
                .expect("the value is a string")
        })
        .collect();
    assert_eq!(&order[..3], [&charger_key, &cable_key, &phone_key]);

    let needed = shop.get(&format!("{search}&status=needed")).await;
    assert_eq!(needed["pagination"]["total"], 1);
    assert_eq!(needed["items"][0]["product"]["key"], cable_key.as_str());
    // The row carries the whole product, price tiers included, ready to print.
    assert_eq!(
        needed["items"][0]["product"]["priceTiers"]
            .as_array()
            .expect("the value is an array")
            .len(),
        2
    );

    // A past job is printed again with today's details: only the named products.
    let picked_uri = format!("/api/labels/coverage?productKeys={cable_key},{phone_key}");
    let picked = shop.get(&picked_uri).await;
    assert_eq!(picked["pagination"]["total"], 2, "{picked}");
    // What the shop paid stays Admin-only.
    for row in picked["items"].as_array().expect("the value is an array") {
        assert!(
            row["product"]["priceTiers"][0]
                .get("minUnitCostCents")
                .is_none()
        );
    }
    let (status, admin_view) = send(&shop.router, "GET", &picked_uri, None, &shop.admin).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        admin_view["data"]["items"][0]["product"]["priceTiers"][0]["minUnitCostCents"].is_number()
    );
    let too_many: Vec<String> = (0..201).map(|i| format!("prod_{i}")).collect();
    let (status, body) = send(
        &shop.router,
        "GET",
        &format!("/api/labels/coverage?productKeys={}", too_many.join(",")),
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn labels_scenario_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn labels_scenario_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}

/// Every role that can see the catalog may read the designs; making or changing
/// designs, previewing, printing, the print history and creating a barcode need
/// inventory write (Admin and Manager).
#[tokio::test]
async fn labels_follow_the_inventory_permissions() {
    let app = common::spawn_app_sqlite().await;
    let shop = Shop::new(&app).await;
    let (status, list) = send(
        &shop.router,
        "GET",
        "/api/labels/templates",
        None,
        &shop.staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");

    let valid = design("Staff attempt", json!([barcode(46.0)]));
    let sample = json!({
        "name": "n", "sku": "s", "category": "c", "priceCents": 100, "code": "2000000000015"
    });
    for (method, uri, body) in [
        ("POST", "/api/labels/templates", Some(valid.clone())),
        ("PUT", "/api/labels/templates/ltpl_x", Some(valid.clone())),
        ("DELETE", "/api/labels/templates/ltpl_x", None),
        (
            "POST",
            "/api/labels/preview",
            Some(json!({ "template": valid, "sample": sample })),
        ),
        (
            "POST",
            "/api/inventory/products/64b0000000000000000000ff/barcode",
            None,
        ),
        (
            "POST",
            "/api/labels/render",
            Some(json!({ "templateKey": PRICE_TAG, "items": [] })),
        ),
        (
            "POST",
            "/api/labels/jobs",
            Some(
                json!({ "templateKey": PRICE_TAG, "items": [], "action": "download",
                         "method": null, "printerName": null, "dpi": null }),
            ),
        ),
        ("GET", "/api/labels/jobs", None),
        ("GET", "/api/labels/jobs/lpj_x", None),
        (
            "PATCH",
            "/api/labels/jobs/lpj_x",
            Some(json!({ "outcome": "sent", "error": null })),
        ),
        ("GET", "/api/labels/coverage", None),
    ] {
        let (status, body) = send(&shop.router, method, uri, body, &shop.staff).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
    }
}

#[tokio::test]
async fn each_shop_has_its_own_label_designs() {
    let app = common::spawn_app_multi_tenant().await;
    let admin_of = |tenant: &str| {
        common::mint_token_for_tenant(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
            Some(tenant),
        )
    };
    let (shop_a, shop_b) = (admin_of("tnt_label_a"), admin_of("tnt_label_b"));

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/labels/templates",
        Some(design("Only for A", json!([barcode(46.0)]))),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let key = body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let names = |list: &Value| -> Vec<String> {
        list["data"]["templates"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .map(|t| {
                t["name"]
                    .as_str()
                    .expect("the value is a string")
                    .to_string()
            })
            .collect()
    };
    let (_, a) = send(&app.router, "GET", "/api/labels/templates", None, &shop_a).await;
    let (_, b) = send(&app.router, "GET", "/api/labels/templates", None, &shop_b).await;
    assert!(names(&a).contains(&"Only for A".to_string()));
    assert!(
        !names(&b).contains(&"Only for A".to_string()),
        "shop B must not see shop A's design"
    );

    // Shop B can neither change nor delete it, and may reuse the name.
    let uri = format!("/api/labels/templates/{key}");
    let (status, _) = send(&app.router, "DELETE", &uri, None, &shop_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &app.router,
        "POST",
        "/api/labels/templates",
        Some(design("Only for A", json!([barcode(46.0)]))),
        &shop_b,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn label_jobs_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    jobs_scenario(&app).await;
}

#[tokio::test]
async fn label_jobs_on_mongo() {
    let app = common::spawn_app_isolated().await;
    jobs_scenario(&app).await;
    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn each_shop_has_its_own_print_history() {
    let app = common::spawn_app_multi_tenant().await;
    let admin_of = |tenant: &str| {
        common::mint_token_for_tenant(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
            Some(tenant),
        )
    };
    let (shop_a, shop_b) = (admin_of("tnt_jobs_a"), admin_of("tnt_jobs_b"));

    let (status, category) = send(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Jobs Cat", "icon": "box", "color": "blue" })),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let category_key = category["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (_, sub) = send(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Items" })),
        &shop_a,
    )
    .await;
    let (status, product) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Shop A cable",
            "categoryKey": category_key,
            "subcategoryKey": sub["data"]["subcategories"][0]["key"], "warranty": { "coverage": { "kind": "none" } },
            "minStockThreshold": 1,
            "autoGenerateBarcode": true,
        })),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let (status, design) = send(
        &app.router,
        "POST",
        "/api/labels/templates",
        Some(design("Barcode only", json!([barcode(46.0)]))),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{design}");

    let (status, pdf) = send_raw(
        &app.router,
        "POST",
        "/api/labels/jobs",
        Some(json!({
            "templateKey": design["data"]["key"],
            "items": [{ "kind": "product", "productKey": product["data"]["key"], "copies": 1, "priceCents": null }],
            "action": "download", "method": null, "printerName": null, "dpi": null,
        })),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    let (_, a) = send(&app.router, "GET", "/api/labels/jobs", None, &shop_a).await;
    let (_, b) = send(&app.router, "GET", "/api/labels/jobs", None, &shop_b).await;
    assert_eq!(a["data"]["pagination"]["total"], 1, "{a}");
    assert_eq!(
        b["data"]["pagination"]["total"], 0,
        "shop B must not see shop A's prints"
    );
    let key = a["data"]["items"][0]["key"]
        .as_str()
        .expect("the value is a string");
    let (status, _) = send(
        &app.router,
        "GET",
        &format!("/api/labels/jobs/{key}"),
        None,
        &shop_b,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    app.mongo().drop().await.ok();
}
