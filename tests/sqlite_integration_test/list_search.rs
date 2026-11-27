//! `?search=` on the other lists, and the user list's role and active filters, on SQLite.

use axum::http::StatusCode;
use serde_json::{Value, json};
use simplebash_pos_backend::clients;
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::core::logging::SqlLogging;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::users;

use crate::common;
use crate::helpers::{
    SqliteTestContext, admin_token, assert_search, create, send_authed, setup_sqlite_app,
};

#[tokio::test]
async fn sqlite_user_list_applies_role_and_search() {
    let ctx = setup_sqlite_app().await;
    let admin = admin_token(&ctx.config);
    let mut keys = Vec::new();
    for (name, username, role) in [
        ("Nimal Silva", "nimal", "manager"),
        ("Kasun Fernando", "kasun", "staff"),
    ] {
        let user = json!({ "name": name, "username": username, "role": role,
                           "password": "Str0ng-Passw0rd!" });
        keys.push(create(&ctx, &admin, "/api/users", user).await);
    }
    let (nimal, kasun) = (&keys[0], &keys[1]);
    let list = ("/api/users", "users");

    assert_search(&ctx, &admin, list, "", &[nimal, kasun]).await;
    assert_search(&ctx, &admin, list, "silva", &[nimal]).await;
    assert_search(&ctx, &admin, list, "KASU", &[kasun]).await;
    assert_search(&ctx, &admin, list, "nobody", &[]).await;
    assert_search(&ctx, &admin, list, "&role=staff", &[kasun]).await;

    // A manager manages staff only, whatever they ask for.
    let manager = common::mint_token(
        &ctx.config,
        Some(Role::Manager),
        simplebash_pos_backend::core::constants::roles::default_permissions(Role::Manager),
    );
    assert_search(&ctx, &manager, list, "", &[kasun]).await;
    assert_search(&ctx, &manager, list, "nimal", &[]).await;
    assert_search(&ctx, &manager, list, "&role=manager", &[]).await;

    // Neither account is the shop's Admin.
    let url = format!("sqlite://{}", ctx.db_path.display());
    let pool = clients::sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the test database opens");
    let found = users::service::find_admin_document(&Db::Sqlite(pool))
        .await
        .expect("the lookup runs");
    assert!(found.is_none(), "a non-admin was taken for the Admin");
}

#[tokio::test]
async fn sqlite_refuses_the_pin_of_a_deactivated_user() {
    let ctx = setup_sqlite_app().await;
    let admin = admin_token(&ctx.config);
    let user = json!({ "name": "Kasun", "username": "kasun", "role": "staff",
                       "password": "Str0ng-Passw0rd!" });
    let (status, created) =
        send_authed(&ctx.router, "POST", "/api/users", Some(user), &admin).await;
    assert!(status.is_success(), "Body: {created}");
    let id = created["data"]["id"].as_str().expect("the id is a string");
    let login = json!({ "username": "kasun", "password": "Str0ng-Passw0rd!" });
    let (status, session) =
        send_authed(&ctx.router, "POST", "/api/auth/login", Some(login), &admin).await;
    assert_eq!(status, StatusCode::OK, "Body: {session}");
    let staff = session["data"]["token"]
        .as_str()
        .expect("the token is a string");
    let pin = json!({ "pin": "4321" });
    let (status, set) = send_authed(
        &ctx.router,
        "POST",
        "/api/auth/set-pin",
        Some(pin.clone()),
        staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {set}");

    let verify = "/api/auth/verify-pin";
    let (status, body) = send_authed(&ctx.router, "POST", verify, Some(pin.clone()), &admin).await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let uri = format!("/api/users/{id}");
    let off = json!({ "isActive": false });
    let (status, body) = send_authed(&ctx.router, "PATCH", &uri, Some(off), &admin).await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let (status, body) = send_authed(&ctx.router, "POST", verify, Some(pin), &admin).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "Body: {body}");
}

#[tokio::test]
async fn sqlite_search_filters_the_contact_lists() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);
    let both = async |uri: &str, first: Value, second: Value| {
        [
            create(&ctx, &token, uri, first).await,
            create(&ctx, &token, uri, second).await,
        ]
    };

    let [jane, kamal] = both(
        "/api/customers",
        json!({ "name": "Jane Doe", "primaryPhone": "0771234567",
                "secondaryPhone": "0112000111", "email": "jane@example.com",
                "address": "12 Galle Road", "contactPerson": "Ruwan" }),
        json!({ "name": "Kamal Perera", "primaryPhone": "0719876543" }),
    )
    .await;
    let list = ("/api/customers", "customers");
    for term in [
        "jane",
        "0771234567",
        "0112000111",
        "example.com",
        "galle",
        "RUWAN",
    ] {
        assert_search(&ctx, &token, list, term, &[&jane]).await;
    }
    assert_search(&ctx, &token, list, "perera", &[&kamal]).await;
    assert_search(&ctx, &token, list, "nobody", &[]).await;
    assert_search(&ctx, &token, list, "J.ne", &[]).await;

    let [acme, lanka] = both(
        "/api/suppliers",
        json!({ "name": "Acme Parts", "address": "12 Galle Road",
                "suppliedCategories": ["Screens"],
                "contactPersons": [
                    { "name": "Ann", "phoneNumbers": ["0112223334"] },
                    { "name": "Bandara", "phoneNumbers": ["0765550000"] },
                ] }),
        json!({ "name": "Lanka Traders", "contactPerson": "Sunil",
                "primaryPhone": "0714445556", "address": "Kandy",
                "suppliedCategories": ["Batteries"] }),
    )
    .await;
    let list = ("/api/suppliers", "suppliers");
    for term in [
        "acme",
        "ann",
        "0112223334",
        "bandara",
        "0765550000",
        "galle",
    ] {
        assert_search(&ctx, &token, list, term, &[&acme]).await;
    }
    assert_search(&ctx, &token, list, "sunil", &[&lanka]).await;
    assert_search(&ctx, &token, list, "nobody", &[]).await;
    assert_search(&ctx, &token, list, "a&category=Batteries", &[&lanka]).await;

    let [sam, mala] = both(
        "/api/employees",
        json!({ "name": "Sam Dias", "phone": "0771112223", "nicOrId": "901234567V",
                "role": "technician", "defaultSplitType": "percentage",
                "defaultSplitValue": 10.0 }),
        json!({ "name": "Mala Silva", "phone": "0702223334", "role": "technician",
                "defaultSplitType": "percentage", "defaultSplitValue": 10.0 }),
    )
    .await;
    let list = ("/api/employees", "employees");
    for term in ["dias", "0771112223", "901234567v"] {
        assert_search(&ctx, &token, list, term, &[&sam]).await;
    }
    assert_search(&ctx, &token, list, "mala", &[&mala]).await;
    assert_search(&ctx, &token, list, "nobody", &[]).await;
}

#[tokio::test]
async fn sqlite_search_filters_the_sales_documents() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);

    let mut customers = Vec::new();
    let mut invoices = Vec::new();
    let mut quotations = Vec::new();
    for (name, phone) in [("Jane Doe", "0771234567"), ("Kamal Perera", "0719876543")] {
        let customer = json!({ "name": name, "primaryPhone": phone });
        let customer_key = create(&ctx, &token, "/api/customers", customer).await;
        let repair = json!({
            "customer": { "customerKey": customer_key },
            "deviceModel": "iPhone 14",
            "issueDescription": "Cracked screen",
            "items": [{ "description": "Repair work", "laborCents": 850000,
                        "warranty": { "coverage": { "kind": "none" } } }],
        });
        let repair_key = create(&ctx, &token, "/api/repairs", repair).await;
        customers.push(customer_key.clone());
        let sale = json!({
            "staff": { "cashierName": "Default Admin" },
            "customer": { "customerKey": customer_key },
            "items": [{ "name": "Screen Replacement", "quantity": 1, "discountCents": 0,
                        "sourceType": "repair", "sourceTicketKey": repair_key,
                        "handover": "now" }],
            "payment": { "paymentMethod": "cash", "isCredit": true },
            "shopProfileSnapshot": { "tradingName": "TechFix Repairs" },
        });
        let (status, sold) = send_authed(
            &ctx.router,
            "POST",
            "/api/billing/sales",
            Some(sale),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "Body: {sold}");
        invoices.push(sold["data"]["invoice"]["key"].clone());
        let quotation = json!({
            "customer": { "customerName": name, "customerPhone": phone },
            "items": [{ "name": "Glass", "unitPriceCents": 2500, "quantity": 1 }],
            "validUntil": "2099-01-01",
            "preparedBy": "Kamal",
        });
        quotations.push(create(&ctx, &token, "/api/quotations", quotation).await);
    }

    for (list, keys) in [
        (("/api/billing/invoices", "invoices"), &invoices),
        (("/api/reports/outstanding", "invoices"), &invoices),
        (("/api/quotations", "quotations"), &quotations),
    ] {
        let (jane, kamal) = (&keys[0], &keys[1]);
        assert_search(&ctx, &token, list, "jane", &[jane]).await;
        assert_search(&ctx, &token, list, "0719876543", &[kamal]).await;
        assert_search(&ctx, &token, list, "nobody", &[]).await;
        assert_search(&ctx, &token, list, "%25", &[]).await;
        assert_search(&ctx, &token, list, "J_ne", &[]).await;
    }
    let list = ("/api/billing/invoices", "invoices");
    assert_search(&ctx, &token, list, "INV-000002", &[&invoices[1]]).await;
    let list = ("/api/reports/outstanding", "invoices");
    assert_search(&ctx, &token, list, "INV-000002", &[&invoices[1]]).await;

    let uri = "/api/reports/outstanding?search=jane";
    let (status, owed) = send_authed(&ctx.router, "GET", uri, None, &token).await;
    assert_eq!(status, StatusCode::OK, "Body: {owed}");
    assert_eq!(owed["data"]["invoices"][0]["customerName"], "Jane Doe");
    assert_eq!(owed["data"]["invoices"][0]["customerPhone"], "0771234567");

    let mut credit_notes = Vec::new();
    for invoice_key in &invoices {
        let returned = json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{ "name": "Screen Replacement", "quantity": 1,
                                "condition": "resalable", "reason": "defective" }],
            "paymentMethod": "cash",
        });
        credit_notes.push(create(&ctx, &token, "/api/billing/credit-notes", returned).await);
    }
    let list = ("/api/billing/credit-notes", "creditNotes");
    let (jane, kamal) = (&credit_notes[0], &credit_notes[1]);
    assert_search(&ctx, &token, list, "jane", &[jane]).await;
    let by_customer = format!(
        "&customerKey={}",
        customers[1].as_str().expect("the key is a string")
    );
    assert_search(&ctx, &token, list, &by_customer, &[kamal]).await;
    assert_search(&ctx, &token, list, "cn-", &[jane, kamal]).await;
    assert_search(&ctx, &token, list, "CN-000002", &[kamal]).await;
    assert_search(&ctx, &token, list, "INV-000001", &[jane]).await;
    assert_search(&ctx, &token, list, "nobody", &[]).await;
    assert_search(&ctx, &token, list, "%25", &[]).await;
}

/// The keys of the customer list, in the order returned.
async fn customer_order(ctx: &SqliteTestContext, token: &str, query: &str) -> Vec<Value> {
    let uri = format!("/api/customers?{query}");
    let (status, body) = send_authed(&ctx.router, "GET", &uri, None, token).await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["customers"]
        .as_array()
        .expect("the list is an array")
        .iter()
        .map(|customer| customer["key"].clone())
        .collect()
}

#[tokio::test]
async fn sqlite_sorts_customers_by_every_offered_field() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);
    let mut keys = Vec::new();
    for (name, phone) in [
        ("Amal", "0773000000"),
        ("Bimal", "0771000000"),
        ("Chamal", "0772000000"),
    ] {
        let customer = json!({ "name": name, "primaryPhone": phone });
        keys.push(create(&ctx, &token, "/api/customers", customer).await);
    }
    let [a, b, c] = [&keys[0], &keys[1], &keys[2]];

    // Bimal buys something, which is also the latest change to any of them.
    let repair = json!({
        "customer": { "customerKey": b },
        "deviceModel": "iPhone 14",
        "issueDescription": "Cracked screen",
        "items": [{ "description": "Repair work", "laborCents": 5000,
                    "warranty": { "coverage": { "kind": "none" } } }],
    });
    let repair_key = create(&ctx, &token, "/api/repairs", repair).await;
    let sale = json!({
        "staff": { "cashierName": "Default Admin" },
        "customer": { "customerKey": b },
        "items": [{ "name": "Repair", "quantity": 1, "discountCents": 0,
                    "sourceType": "repair", "sourceTicketKey": repair_key,
                    "handover": "now" }],
        "payment": { "paymentMethod": "cash", "isCredit": false,
                     "amountReceivedCents": 5000 },
        "shopProfileSnapshot": { "tradingName": "TechFix Repairs" },
    });
    let (status, sold) = send_authed(
        &ctx.router,
        "POST",
        "/api/billing/sales",
        Some(sale),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {sold}");

    // Last change first is the default.
    let expected = [b, c, a].map(Value::clone);
    assert_eq!(customer_order(&ctx, &token, "").await, expected);
    let ascending = customer_order(&ctx, &token, "sortBy=updatedAt&sortOrder=asc").await;
    assert_eq!(ascending, [a, c, b].map(Value::clone));
    let by_phone = customer_order(&ctx, &token, "sortBy=primaryPhone&sortOrder=asc").await;
    assert_eq!(by_phone, [b, c, a].map(Value::clone));
    let by_purchases = customer_order(&ctx, &token, "sortBy=totalPurchasesCents").await;
    assert_eq!(by_purchases[0], *b);
}
