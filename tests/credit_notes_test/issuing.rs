//! Issuing tests.

use axum::http::StatusCode;

use serde_json::{Value, json};

use crate::common;
use crate::helpers::{
    admin_token, backdate_invoice, complete_simple_sale, get_product, seed_product, send_authed,
    staff_token,
};

async fn get_product_movements(app: &common::TestApp, id: &str) -> Vec<Value> {
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{id}/movements"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["movements"]
        .as_array()
        .expect("the value is an array")
        .clone()
}

async fn seed_customer(app: &common::TestApp) -> (String, String) {
    let token = staff_token(&app.config);
    let unique_phone = format!(
        "+9477{:07}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .subsec_nanos()
            % 10_000_000
    );
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Credit Note Test Customer",
            "primaryPhone": unique_phone,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    (
        body["data"]["id"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        body["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    )
}

async fn get_customer(app: &common::TestApp, key: &str) -> Value {
    let token = staff_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"].clone()
}

#[tokio::test]
async fn credit_note_resalable_restocks_inventory_and_issues_cash_refund() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_id, prod_key, price_cents) = seed_product(&app, "Refund Test Phone", 5000, 10).await;
    let (_cust_id, cust_key) = seed_customer(&app).await;

    let invoice_key =
        complete_simple_sale(&app, &token, &prod_key, 2, price_cents, Some(&cust_key)).await;

    let prod_after_sale = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_sale["stockQuantity"], 8);
    let cust_after_sale = get_customer(&app, &cust_key).await;
    assert_eq!(cust_after_sale["totalPurchasesCents"], price_cents * 2);

    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "resalable",
                "notes": "Customer reported minor screen defect"
            }],
            "paymentMethod": "cash",
            "notes": "Refund processed at counter"
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert!(
        cn_data["key"]
            .as_str()
            .expect("the value is a string")
            .starts_with("cn_")
    );
    assert!(
        cn_data["creditNoteNumber"]
            .as_str()
            .expect("the value is a string")
            .starts_with("CN-")
    );
    assert_eq!(cn_data["returnSubtotalCents"], price_cents);
    assert_eq!(cn_data["netRefundCents"], price_cents);
    assert_eq!(cn_data["refundCashCents"], price_cents);
    assert_eq!(cn_data["balanceReductionCents"], 0);
    assert_eq!(cn_data["status"], "resolved");
    assert!(
        !cn_data["refundPaymentKeys"]
            .as_array()
            .expect("the value is an array")
            .is_empty()
    );

    let prod_after_return = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_return["stockQuantity"], 9);
    let cust_after_return = get_customer(&app, &cust_key).await;
    assert_eq!(cust_after_return["totalPurchasesCents"], price_cents);

    let (inv_status, inv_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(inv_status, StatusCode::OK);
    let inv_data = &inv_body["data"];
    assert_eq!(inv_data["refundedCents"], price_cents);
    assert_eq!(inv_data["items"][0]["returnedQuantity"], 1);
    assert_eq!(inv_data["creditNoteCount"], 1);

    let movements = get_product_movements(&app, &prod_id).await;
    assert!(
        movements
            .iter()
            .any(|m| m["type"] == "return_restock" && m["quantityDelta"] == 1),
        "expected a return_restock +1 movement, got: {movements:?}"
    );
}

#[tokio::test]
async fn credit_note_requires_disposition_when_damaged_and_maps_stock_movement_types() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_id, prod_key, price_cents) = seed_product(&app, "Damaged Cable", 1500, 10).await;
    let invoice_key = complete_simple_sale(&app, &token, &prod_key, 2, price_cents, None).await;

    // Damaged with no disposition must be rejected.
    let (missing_status, missing_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "damaged"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(missing_status, StatusCode::BAD_REQUEST, "{missing_body}");

    // Damaged + write_off_scrap -> ReturnWriteOff, no quantity change, audit-only.
    let (wo_status, wo_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "damaged",
                "disposition": "write_off_scrap",
                "notes": "Cable cut, discarded"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(wo_status, StatusCode::OK, "Credit note body: {wo_body}");

    let prod_after_writeoff = get_product(&app, &prod_id).await;
    assert_eq!(
        prod_after_writeoff["stockQuantity"], 8,
        "write-off must not restock"
    );

    let movements = get_product_movements(&app, &prod_id).await;
    assert!(
        movements
            .iter()
            .any(|m| m["type"] == "return_write_off" && m["quantityDelta"] == 0),
        "expected a return_write_off audit-only movement, got: {movements:?}"
    );

    // Damaged + return_to_supplier -> ReturnSupplierRma, no quantity change, audit-only.
    let (rma_status, rma_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "warranty_claim",
                "condition": "damaged",
                "disposition": "return_to_supplier"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(rma_status, StatusCode::OK, "Credit note body: {rma_body}");
    let prod_after_rma = get_product(&app, &prod_id).await;
    assert_eq!(
        prod_after_rma["stockQuantity"], 8,
        "supplier RMA must not restock"
    );
    let movements = get_product_movements(&app, &prod_id).await;
    assert!(
        movements
            .iter()
            .any(|m| m["type"] == "return_supplier_rma" && m["quantityDelta"] == 0),
        "expected a return_supplier_rma audit-only movement, got: {movements:?}"
    );
}

#[tokio::test]
async fn credit_note_with_exchange_items_customer_pays_extra() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_a_id, prod_a_key, prod_a_price) = seed_product(&app, "Product A", 1000, 10).await;
    let (prod_b_id, prod_b_key, prod_b_price) = seed_product(&app, "Product B", 2500, 10).await;

    let invoice_key = complete_simple_sale(&app, &token, &prod_a_key, 1, prod_a_price, None).await;

    // Return Product A ($10) and exchange for Product B ($25): difference = 1500, customer pays extra.
    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_a_key,
                "quantity": 1,
                "reason": "customer_changed_mind",
                "condition": "resalable"
            }],
            "exchangeItems": [{
                "productKey": prod_b_key,
                "sellingPriceCents": prod_b_price,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "paymentMethod": "card"
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert_eq!(cn_data["returnSubtotalCents"], 1000);
    assert_eq!(cn_data["exchangeSubtotalCents"], 2500);
    assert_eq!(cn_data["netRefundCents"], -1500);
    assert_eq!(
        cn_data["exchangeItems"]
            .as_array()
            .expect("the value is an array")
            .len(),
        1
    );
    assert!(
        cn_data["exchangeReference"]
            .as_str()
            .expect("the value is a string")
            .starts_with("exg_")
    );

    let prod_a = get_product(&app, &prod_a_id).await;
    assert_eq!(prod_a["stockQuantity"], 10);
    let prod_b = get_product(&app, &prod_b_id).await;
    assert_eq!(prod_b["stockQuantity"], 9);
}

#[tokio::test]
async fn credit_note_with_exchange_items_customer_gets_cashback() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_a_id, prod_a_key, prod_a_price) = seed_product(&app, "Cheap Item", 1000, 10).await;
    let (prod_b_id, prod_b_key, prod_b_price) =
        seed_product(&app, "Expensive Item", 3000, 10).await;

    let invoice_key = complete_simple_sale(&app, &token, &prod_b_key, 1, prod_b_price, None).await;

    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_b_key,
                "quantity": 1,
                "reason": "wrong_item",
                "condition": "resalable"
            }],
            "exchangeItems": [{
                "productKey": prod_a_key,
                "sellingPriceCents": prod_a_price,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert_eq!(cn_data["returnSubtotalCents"], 3000);
    assert_eq!(cn_data["exchangeSubtotalCents"], 1000);
    assert_eq!(cn_data["netRefundCents"], 2000);
    assert_eq!(cn_data["refundCashCents"], 2000);

    let prod_b = get_product(&app, &prod_b_id).await;
    assert_eq!(prod_b["stockQuantity"], 10);
    let prod_a = get_product(&app, &prod_a_id).await;
    assert_eq!(prod_a["stockQuantity"], 9);
}

#[tokio::test]
async fn credit_note_refund_is_capped_at_amount_actually_paid_on_partially_paid_invoice() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    // Rs. 250.00 total item, sold on credit (nothing paid at sale time).
    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Credit Sale Item", 25000, 10).await;
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "productKey": prod_key,
                "sellingPriceCents": price_cents,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": { "paymentMethod": "credit", "isCredit": true },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK, "{sale_body}");
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string");

    // Customer pays Rs. 150.00 as a deposit -> PartiallyPaid, Rs. 100.00 still outstanding.
    let (pay_status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        Some(json!({ "amountCents": 15000, "paymentMethod": "cash" })),
        &token,
    )
    .await;
    assert_eq!(pay_status, StatusCode::OK);

    let (inv_status, inv_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(inv_status, StatusCode::OK);
    assert_eq!(inv_body["data"]["status"], "partially_paid");

    // Full return of the item (value Rs. 250.00) — only Rs. 150.00 was actually paid,
    // so refund_cash_cents must cap at 15000 and the remaining 10000 reduces the balance.
    let (preview_status, preview) = send_authed(
        &app.router,
        "POST",
        "/api/calc/credit-note",
        Some(json!({
            "invoiceKey": invoice_key,
            "returned": [{ "lineIndex": 0, "quantity": 1 }],
            "noReceipt": null,
            "exchanged": []
        })),
        &token,
    )
    .await;
    assert_eq!(preview_status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["netRefundCents"], price_cents);
    assert_eq!(preview["data"]["refundCashCents"], 15000);
    assert_eq!(
        preview["data"]["balanceReductionCents"],
        price_cents - 15000
    );
    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "customer_changed_mind",
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert_eq!(cn_data["netRefundCents"], price_cents);
    assert_eq!(cn_data["refundCashCents"], 15000);
    assert_eq!(cn_data["balanceReductionCents"], price_cents - 15000);
}

#[tokio::test]
async fn credit_note_refund_breakdown_defaults_proportionally_for_split_paid_invoice() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    // Rs. 100.00 total, split 60/40 cash/card.
    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Split Pay Item", 10000, 10).await;
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "productKey": prod_key,
                "sellingPriceCents": price_cents,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": {
                "paymentMethod": "split",
                "isCredit": false,
                "splitPayments": [
                    { "method": "cash", "amountCents": 6000 },
                    { "method": "card", "amountCents": 4000 }
                ]
            },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK, "{sale_body}");
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string");

    let (_, preview) = send_authed(
        &app.router,
        "POST",
        "/api/calc/credit-note",
        Some(json!({
            "invoiceKey": invoice_key,
            "returned": [{ "lineIndex": 0, "quantity": 1 }],
            "noReceipt": null,
            "exchanged": []
        })),
        &token,
    )
    .await;
    let preview_legs = preview["data"]["refundLegs"]
        .as_array()
        .expect("the value is an array")
        .clone();

    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "customer_changed_mind",
                "condition": "resalable"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let breakdown = cn_body["data"]["refundBreakdown"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(breakdown.len(), 2);
    // The preview showed exactly these legs.
    for leg in breakdown {
        assert!(
            preview_legs
                .iter()
                .any(|p| p["method"] == leg["method"] && p["amountCents"] == leg["amountCents"])
        );
    }
    let total: i64 = breakdown
        .iter()
        .map(|l| l["amountCents"].as_i64().expect("the value is an integer"))
        .sum();
    assert_eq!(total, 10000);
    let cash_leg = breakdown
        .iter()
        .find(|l| l["method"] == "cash")
        .expect("a match exists");
    let card_leg = breakdown
        .iter()
        .find(|l| l["method"] == "card")
        .expect("a match exists");
    assert_eq!(cash_leg["amountCents"], 6000);
    assert_eq!(card_leg["amountCents"], 4000);
}

#[tokio::test]
async fn credit_note_no_receipt_requires_admin_and_override_reason_priced_at_current_selling_price()
{
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);

    let (_prod_id, prod_key, current_price) = seed_product(&app, "No Receipt Item", 4500, 10).await;

    // Staff cannot even attempt a no-receipt return.
    let (staff_status, staff_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "noReceipt": true,
            "overrideReason": "Customer insists item was bought here",
            "returnedItems": [{
                "productKey": prod_key,
                "sellingPriceCents": current_price,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &staff,
    )
    .await;
    assert_eq!(staff_status, StatusCode::FORBIDDEN, "{staff_body}");
    assert_eq!(staff_body["code"], "MANAGER_OVERRIDE_REQUIRED");

    // Admin without an override reason is rejected too.
    let (no_reason_status, _) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "noReceipt": true,
            "returnedItems": [{
                "productKey": prod_key,
                "sellingPriceCents": current_price,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &admin,
    )
    .await;
    assert_eq!(no_reason_status, StatusCode::BAD_REQUEST);

    // Admin + override reason succeeds, valued at current selling price.
    let (ok_status, ok_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "noReceipt": true,
            "overrideReason": "Manager verified purchase from receipt-less walk-in",
            "returnedItems": [{
                "productKey": prod_key,
                "sellingPriceCents": current_price,
                "quantity": 1,
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &admin,
    )
    .await;
    assert_eq!(ok_status, StatusCode::OK, "{ok_body}");
    let cn_data = &ok_body["data"];
    assert_eq!(cn_data["noReceipt"], true);
    assert_eq!(cn_data["isManagerOverride"], true);
    assert_eq!(cn_data["returnSubtotalCents"], current_price);
    assert!(cn_data["invoiceKey"].is_null());
}

#[tokio::test]
async fn credit_note_past_return_window_is_blocked_unless_manager_overrides() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);

    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Old Sale Item", 3000, 10).await;
    let invoice_key = complete_simple_sale(&app, &staff, &prod_key, 1, price_cents, None).await;
    backdate_invoice(app.mongo(), &invoice_key, 45).await; // default window is 30 days

    // No override -> blocked.
    let (blocked_status, blocked_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &staff,
    )
    .await;
    assert_eq!(blocked_status, StatusCode::CONFLICT, "{blocked_body}");
    assert_eq!(blocked_body["code"], "RETURN_WINDOW_EXPIRED");

    // Override reason present but caller is Staff, not Admin -> forbidden.
    let (staff_override_status, _) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "overrideReason": "Regular customer, approved verbally",
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &staff,
    )
    .await;
    assert_eq!(staff_override_status, StatusCode::FORBIDDEN);

    // Admin + override reason succeeds.
    let (admin_status, admin_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "overrideReason": "Manager approved as a goodwill exception",
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &admin,
    )
    .await;
    assert_eq!(admin_status, StatusCode::OK, "{admin_body}");
    assert_eq!(admin_body["data"]["isManagerOverride"], true);
}

#[tokio::test]
async fn charge_line_is_refunded_without_touching_stock_and_voids_cleanly() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let sale = |amount: i64| {
        json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "name": "Delivery fee",
                "unitPriceCents": 1500,
                "quantity": 2,
                "discountCents": 0,
                "sourceType": "charge"
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": amount },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })
    };
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(sale(3000)),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let invoice_key = body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // A charge is not a physical item: damaged / disposition choices make no sense on it.
    let damaged = json!({
        "invoiceKey": invoice_key,
        "returnedItems": [{
            "name": "Delivery fee",
            "quantity": 1,
            "reason": "defective",
            "condition": "damaged",
            "disposition": "write_off_scrap"
        }],
        "paymentMethod": "cash"
    });
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(damaged),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");

    let refund = json!({
        "invoiceKey": invoice_key,
        "returnedItems": [{
            "name": "Delivery fee",
            "quantity": 1,
            "reason": "defective",
            "condition": "resalable"
        }],
        "paymentMethod": "cash"
    });
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(refund),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["returnSubtotalCents"], 1500);
    assert_eq!(body["data"]["netRefundCents"], 1500);
    assert_eq!(body["data"]["refundCashCents"], 1500);
    assert_eq!(body["data"]["returnedItems"][0]["sourceType"], "charge");
    assert!(body["data"]["returnedItems"][0]["productKey"].is_null());

    // A second sale on the same shape, voided, reverses nothing and warns about nothing.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(sale(3000)),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let second_key = body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let admin = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{second_key}/void"),
        Some(json!({ "reason": "Entered by mistake" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["status"], "voided", "Body: {body}");
    assert_eq!(
        body["message"], "Invoice voided successfully",
        "a charge has nothing to put back, so no warning: {body}"
    );
}
