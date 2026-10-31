//! Purchases and contacts tests.

use axum::http::StatusCode;

use mongodb::bson::DateTime as BsonDateTime;
use serde_json::json;

use simplebash_pos_backend::core::id::generate_id;
use simplebash_pos_backend::domain::suppliers::ContactPerson;
use simplebash_pos_backend::domain::users::Role;

use simplebash_pos_backend::modules::suppliers::model::SupplierDocument;

use crate::common;
use crate::helpers::{admin_token, create_product, create_supplier, send, send_authed};
use uuid::Uuid;

/// Inserts a minimal `SupplierDocument` directly, so the stats test can
/// control `contact_person`/`supplied_categories`/`deleted_at` precisely.
async fn seed_supplier_with(
    db: &mongodb::Database,
    contact_person: &str,
    supplied_categories: Vec<String>,
    deleted: bool,
) {
    let key = generate_id("sup");
    let now = BsonDateTime::now();
    db.collection::<SupplierDocument>("suppliers")
        .insert_one(SupplierDocument {
            id: None,
            key,
            name: format!("Stats Test Supplier {}", Uuid::new_v4()),
            contact_person: contact_person.to_string(),
            primary_phone: "0770000000".to_string(),
            secondary_phone: None,
            contact_persons: vec![ContactPerson {
                name: contact_person.to_string(),
                phone_numbers: vec!["0770000000".to_string()],
            }],
            address: "123 Test St".to_string(),
            supplied_categories,
            email: None,
            notes: None,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: if deleted { Some(now) } else { None },
            updated_by_device: None,
        })
        .await
        .expect("failed to seed supplier");
}

// ============================================================================
// Purchases
// ============================================================================

#[tokio::test]
async fn record_purchase_increments_stock_and_writes_movement() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (product_id, product_key) = create_product(&app, 10).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": "2026-01-15T10:00:00Z",
            "referenceNo": "INV-TEST-1",
            "lines": [{
                "productKey": product_key,
                "quantity": 25,
                "lineTotalCents": 25 * 1500,
                "sellingPriceCents": 2500,
            }],
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["totalCents"], 25 * 1500);
    assert_eq!(body["data"]["referenceNo"], "INV-TEST-1");
    assert_eq!(body["data"]["supplier"]["key"], supplier_key);
    let line = &body["data"]["lines"][0];
    assert_eq!(line["landedTotalCents"], 25 * 1500);
    assert_eq!(line["lineTotalCents"], 25 * 1500);
    assert_eq!(line["landedUnitCostCents"], 1500);
    assert_eq!(line["product"]["key"], product_key);
    assert_eq!(line["receiptKey"], body["data"]["key"]);
    let purchase_key = line["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &admin_token(&app.config),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["stockQuantity"], 35);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{product_id}/movements"),
        None,
        &admin_token(&app.config),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let movements = body["data"]["movements"]
        .as_array()
        .expect("the value is an array");
    let movement = movements
        .iter()
        .find(|m| m["referenceId"] == json!(purchase_key))
        .expect("purchase movement not found");
    assert_eq!(movement["type"], "purchase_receipt");
    assert_eq!(movement["quantityDelta"], 25);
}

#[tokio::test]
async fn purchases_endpoints_require_admin_role() {
    let app = common::spawn_app().await;

    let (status, body) = send(&app.router, "GET", "/api/purchases", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let non_admin = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = send_authed(&app.router, "GET", "/api/purchases", None, &non_admin).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ADMIN_REQUIRED");
}

#[tokio::test]
async fn record_purchase_requires_auth_and_validates_quantity() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    let payload = json!({
        "supplierKey": supplier_key,
        "date": "2026-01-15T10:00:00Z",
        "lines": [{
            "productKey": product_key,
            "quantity": 5,
            "lineTotalCents": 5 * 1000,
            "sellingPriceCents": 2500,
        }],
    });

    let (status, _) = send(
        &app.router,
        "POST",
        "/api/purchase-receipts",
        Some(payload.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let mut invalid = payload.clone();
    invalid["lines"][0]["quantity"] = json!(0);
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/purchase-receipts",
        Some(invalid),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let mut empty = payload;
    empty["lines"] = json!([]);
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/purchase-receipts",
        Some(empty),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Same optional-filter contract as
/// `list_links_without_keys_returns_paginated_collection`: an unfiltered
/// `GET /purchases` is a full-collection sync pull, not a 400.
#[tokio::test]
async fn list_purchases_without_keys_returns_paginated_collection() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": "2026-01-15T10:00:00Z",
            "lines": [{
                "productKey": product_key,
                "quantity": 3,
                "lineTotalCents": 3 * 1500,
                "sellingPriceCents": 2500,
            }],
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = send_authed(&app.router, "GET", "/api/purchases", None, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let purchases = body["data"]["purchases"]
        .as_array()
        .expect("the value is an array");
    assert!(purchases.len() <= 20, "default page size is 20: {body}");
    assert!(
        purchases
            .iter()
            .all(|purchase| purchase["supplierKey"].is_string()),
        "{body}"
    );
    assert_eq!(body["data"]["pagination"]["page"], 1, "{body}");
    assert_eq!(body["data"]["pagination"]["limit"], 20, "{body}");
    assert!(
        body["data"]["pagination"]["total"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 1,
        "the purchase recorded above must be counted: {body}"
    );
}

// ============================================================================
// Cross-module guard
// ============================================================================

#[tokio::test]
async fn deleting_a_supplier_with_purchase_history_is_blocked() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": "2026-01-15T10:00:00Z",
            "lines": [{
                "productKey": product_key,
                "quantity": 5,
                "lineTotalCents": 5 * 1000,
                "sellingPriceCents": 2500,
            }],
        })),
        &token,
    )
    .await;

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/suppliers/{supplier_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "SUPPLIER_HAS_PURCHASES");
}

#[tokio::test]
async fn suppliers_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send(&app.router, "GET", "/api/suppliers/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn suppliers_stats_endpoint_returns_totals_categories_and_direct_contacts() {
    // `spawn_app`'s DB is shared across the suite, and `/suppliers/stats`
    // aggregates the WHOLE collection with no filter — connect an isolated
    // throwaway database instead, same as the billing/repairs/print-jobs
    // stats tests.
    let db_name = format!("jtstats_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let db = simplebash_pos_backend::clients::mongo::connect(&common::mongo_uri(), &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    seed_supplier_with(
        &db,
        "Kasun Silva",
        vec!["Phone Parts".to_string(), "Cables".to_string()],
        false,
    )
    .await;
    seed_supplier_with(&db, "", vec!["Phone Parts".to_string()], false).await;
    seed_supplier_with(&db, "Nadeesha Perera", vec!["Screens".to_string()], false).await;
    // Soft-deleted — must be excluded from both counts.
    seed_supplier_with(&db, "Deleted Contact", vec!["Batteries".to_string()], true).await;

    let db_handle = simplebash_pos_backend::clients::db::Db::from_mongo(db.clone());
    let stats = simplebash_pos_backend::modules::suppliers::service::get_supplier_stats(&db_handle)
        .await
        .expect("get_supplier_stats should succeed");

    assert_eq!(
        stats.total_suppliers, 3,
        "the soft-deleted supplier must be excluded"
    );
    assert_eq!(stats.direct_contacts_count, 2);
    // `supply_categories_count` reuses `distinct_supplied_categories`, the
    // same repository fn `GET /suppliers/categories` already uses — like
    // that endpoint, it's unfiltered by `deleted_at`, so the deleted
    // supplier's "Batteries" tag still counts here.
    assert_eq!(
        stats.supply_categories_count, 4,
        "Phone Parts, Cables, Screens, Batteries"
    );

    db.drop().await.ok();
}

#[tokio::test]
async fn create_supplier_with_multiple_contact_persons_and_up_to_two_phones() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let payload = json!({
        "name": format!("Multi Contact Supplier {}", Uuid::new_v4()),
        "contactPersons": [
            {
                "name": "Ranjith Silva",
                "phoneNumbers": ["077 123 4567", "011 234 5678"]
            },
            {
                "name": "Sunil Perera",
                "phoneNumbers": ["071 999 8888"]
            }
        ],
        "address": "45 Pettah Street, Colombo",
        "suppliedCategories": ["Displays", "Batteries"],
        "email": "contact@multisup.lk",
    });

    let (status, body) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let data = &body["data"];
    assert_eq!(data["contactPerson"], "Ranjith Silva");
    assert_eq!(data["primaryPhone"], "077 123 4567");
    assert_eq!(data["secondaryPhone"], "011 234 5678");

    let cps = data["contactPersons"]
        .as_array()
        .expect("contactPersons array");
    assert_eq!(cps.len(), 2);
    assert_eq!(cps[0]["name"], "Ranjith Silva");
    assert_eq!(
        cps[0]["phoneNumbers"],
        json!(["077 123 4567", "011 234 5678"])
    );
    assert_eq!(cps[1]["name"], "Sunil Perera");
    assert_eq!(cps[1]["phoneNumbers"], json!(["071 999 8888"]));
}

#[tokio::test]
async fn create_supplier_enforces_maximum_two_phones_per_contact_person() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let payload = json!({
        "name": format!("Too Many Phones Supplier {}", Uuid::new_v4()),
        "contactPersons": [
            {
                "name": "Too Many Phones",
                "phoneNumbers": ["077 111 2222", "077 333 4444", "077 555 6666"]
            }
        ],
        "address": "Colombo",
        "suppliedCategories": ["Displays"],
    });

    let (status, body) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("at most 2 phone numbers"),
        "error message must mention maximum 2 phone numbers: {body}"
    );
}

#[tokio::test]
async fn create_supplier_enforces_at_least_one_phone_per_contact_person() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let payload = json!({
        "name": format!("No Phone Supplier {}", Uuid::new_v4()),
        "contactPersons": [
            {
                "name": "No Phone Contact",
                "phoneNumbers": []
            }
        ],
        "address": "Colombo",
        "suppliedCategories": ["Displays"],
    });

    let (status, body) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("at least one phone number"),
        "error message must mention at least one phone number: {body}"
    );
}

#[tokio::test]
async fn create_supplier_enforces_contact_person_name_required() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let payload = json!({
        "name": format!("Empty Contact Name Supplier {}", Uuid::new_v4()),
        "contactPersons": [
            {
                "name": "   ",
                "phoneNumbers": ["077 123 4567"]
            }
        ],
        "address": "Colombo",
        "suppliedCategories": ["Displays"],
    });

    let (status, body) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("name is required"),
        "error message must mention name is required: {body}"
    );
}

#[tokio::test]
async fn search_suppliers_finds_by_secondary_contact_name_or_phone() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let unique = Uuid::new_v4().simple().to_string();

    let sec_name = format!("SecContact{unique}");
    let sec_phone = format!("071{:07}", (Uuid::new_v4().as_u128() % 10_000_000));
    let supplier_name = format!("SearchSupplier{unique}");

    let payload = json!({
        "name": supplier_name,
        "contactPersons": [
            {
                "name": "First Contact",
                "phoneNumbers": ["077 000 1111"]
            },
            {
                "name": sec_name,
                "phoneNumbers": [sec_phone.clone()]
            }
        ],
        "address": "Somewhere",
        "suppliedCategories": ["Parts"],
    });

    let (status, _) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::CREATED);

    // Search by secondary contact person name
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers?search={sec_name}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let suppliers = body["data"]["suppliers"]
        .as_array()
        .expect("the value is an array");
    assert!(
        suppliers.iter().any(|s| s["name"] == supplier_name),
        "expected {supplier_name} when searching by secondary contact name: {body}"
    );

    // Search by secondary contact person phone
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers?search={sec_phone}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let suppliers = body["data"]["suppliers"]
        .as_array()
        .expect("the value is an array");
    assert!(
        suppliers.iter().any(|s| s["name"] == supplier_name),
        "expected {supplier_name} when searching by secondary contact phone: {body}"
    );
}
