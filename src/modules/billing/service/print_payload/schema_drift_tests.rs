use super::build_a4_invoice_data::build_a4_invoice_data;
use super::build_credit_note_data::build_credit_note_data;
use super::build_quotation_data::build_quotation_data;
use super::build_thermal_receipt_data::build_thermal_receipt_data;
use super::build_z_report_data::build_z_report_data;

use crate::domain::sessions::ZReportResponse;

use crate::domain::billing::{CreditNote, Invoice, InvoiceStatus};
use serde_json::json;

/// Every `<name>.schema.json` the sibling document-server publishes for a
/// *document kind* (`x-document-kind` in the schema), as `(title, schema)`.
/// The payload builders feed **all** templates of a kind, so each one must
/// be checked, not just the first. Returns an empty list (with a notice)
/// when that repository isn't checked out next to this one, so the suite
/// stays runnable in a backend-only checkout — where it prints a notice
/// instead of silently passing an empty guarantee.
fn load_schemas_of_kind(kind: &str) -> Vec<(String, serde_json::Value)> {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../document-server/templates/documents"
    );
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("skipping schema drift check for {kind}: no document-server checkout at {dir}");
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".schema.json") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(schema) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        if schema.get("x-document-kind").and_then(|k| k.as_str()) == Some(kind) {
            let title = schema
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("(untitled)")
                .to_string();
            found.push((title, schema));
        }
    }
    if found.is_empty() {
        panic!("no template in {dir} declares x-document-kind {kind:?}");
    }
    found
}

/// Panics with every violation listed when `payload` doesn't satisfy the
/// published contract of **any** template of this kind — the same check
/// `DocumentServerClient::render_template` enforces at request time, run
/// here against the *real* sidecar files.
fn assert_matches_schema(kind: &str, payload: &serde_json::Value) {
    for (title, schema) in load_schemas_of_kind(kind) {
        let validator = jsonschema::validator_for(&schema).expect("published schema compiles");
        let violations: Vec<String> = validator
            .iter_errors(payload)
            .map(|err| {
                let path = err.instance_path().to_string();
                if path.is_empty() {
                    err.to_string()
                } else {
                    format!("at '{path}': {err}")
                }
            })
            .collect();
        assert!(
            violations.is_empty(),
            "{kind} payload drifted from the published data schema of {title:?}:\n  {}",
            violations.join("\n  ")
        );
    }
}

fn cash_sale_invoice() -> Invoice {
    serde_json::from_value(json!({
        "id": "64b0000000000000000000a1",
        "key": "inv_testcash",
        "invoiceNumber": "INV-000001",
        "customerKey": "cust_test",
        "customerNameSnapshot": "Kasun Silva",
        "customerPhoneSnapshot": "077 123 4567",
        "customerAddressSnapshot": "45 Lake Road, Colombo 06",
        "cashierId": "usr_admin",
        "cashierNameSnapshot": "Default Admin",
        "items": [
            {
                "productKey": "prod_test",
                "name": "Screen Protector - iPhone 14",
                "sku": "ACC-SP-014",
                "unitPriceCents": 15000,
                "quantity": 2,
                "discountCents": 0,
                "totalCents": 30000,
                "sourceType": "retail"
            },
            {
                "name": "Screen Replacement",
                "unitPriceCents": 85000,
                "quantity": 1,
                "discountCents": 5000,
                "totalCents": 80000,
                "sourceType": "repair",
                "sourceTicketKey": "rep_test",
                "sourceTicketNumber": "REP-000001",
                "assignedEmployeeName": "Ruwan Fernando"
            }
        ],
        "subtotalCents": 110000,
        "discountType": "percentage",
        "discountValue": 5.0,
        "discountCents": 5500,
        "totalCents": 104500,
        "paymentMethod": "cash",
        "isCredit": false,
        "amountReceivedCents": 110000,
        "changeDueCents": 5500,
        "status": "paid",
        "isOverdue": false,
        "shopProfileSnapshot": {
            "tradingName": "TechFix Repairs",
            "legalName": "TechFix Repairs (Pvt) Ltd",
            "primaryPhone": "011 234 5678",
            "secondaryPhone": "",
            "addressLines": ["123 Galle Road", "Colombo 04"],
            "vatRate": 0.0,
            "receiptFooterText": "Thank you!"
        },
        "createdAt": "2026-08-23T09:00:00Z",
        "updatedAt": "2026-08-23T09:00:00Z"
    }))
    .expect("cash-sale invoice fixture deserializes")
}

#[test]
fn thermal_receipt_payload_matches_published_schema() {
    let invoice = cash_sale_invoice();
    let payload =
        build_thermal_receipt_data(&invoice, 80, invoice.amount_received_cents.unwrap_or(0));
    assert_matches_schema("thermal-receipt", &payload);
}

#[test]
fn a4_invoice_payload_matches_published_schema() {
    let invoice = cash_sale_invoice();
    let payload = build_a4_invoice_data(
        &invoice,
        "ORIGINAL — CUSTOMER COPY",
        false,
        invoice.amount_received_cents.unwrap_or(0),
    );
    assert_matches_schema("a4-invoice", &payload);
}

// The invoice shows "amount received -> change -> total due", so the A4 payload
// must carry all four numbers and a paid invoice must owe nothing.
#[test]
fn a4_invoice_payload_carries_amount_given_change_and_what_is_still_owed() {
    let invoice = cash_sale_invoice();
    let payload = build_a4_invoice_data(&invoice, "ORIGINAL", false, 104_500);
    assert_eq!(payload["totalCents"], 104_500);
    assert_eq!(payload["tenderedAmountCents"], 110_000);
    assert_eq!(payload["changeDueCents"], 5_500);
    assert_eq!(payload["amountPaidCents"], 104_500);
    assert_eq!(payload["balanceDueCents"], 0);
    assert_matches_schema("a4-invoice", &payload);
}

#[test]
fn a4_credit_invoice_owes_the_balance_and_gives_no_change() {
    let mut invoice = cash_sale_invoice();
    invoice.is_credit = true;
    invoice.amount_received_cents = Some(40_000);
    invoice.change_due_cents = None;
    let payload = build_a4_invoice_data(&invoice, "ORIGINAL", false, 40_000);
    assert_eq!(payload["tenderedAmountCents"], 40_000);
    assert_eq!(payload["changeDueCents"], 0);
    assert_eq!(payload["amountPaidCents"], 40_000);
    assert_eq!(payload["balanceDueCents"], 64_500);
    assert_matches_schema("a4-invoice", &payload);
}

#[test]
fn a4_invoice_carries_the_shop_logo_as_logo_url() {
    // No `logoBase64` in the snapshot -> empty string (the "no logo"
    // signal document-server understands), still schema-valid.
    let mut invoice = cash_sale_invoice();
    let payload = build_a4_invoice_data(
        &invoice,
        "ORIGINAL — CUSTOMER COPY",
        false,
        invoice.amount_received_cents.unwrap_or(0),
    );
    assert_eq!(payload["logoUrl"], "");
    assert_matches_schema("a4-invoice", &payload);

    // A shop logo (a data: URI) is passed straight through as `logoUrl`.
    let logo = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    invoice.shop_profile_snapshot["logoBase64"] = json!(logo);
    let payload = build_a4_invoice_data(
        &invoice,
        "ORIGINAL — CUSTOMER COPY",
        false,
        invoice.amount_received_cents.unwrap_or(0),
    );
    assert_eq!(payload["logoUrl"], logo);
    assert_matches_schema("a4-invoice", &payload);
}

/// Every lifecycle status must pass the published enum — the schema's
/// `status` enum drifted once (`partially_paid`/`voided`/`closed` were
/// rejected), which only surfaced as a customer-facing 422 on the real
/// document-server because tests used a mock with minimal schemas.
#[test]
fn a4_invoice_covers_every_lifecycle_status() {
    for status in [
        InvoiceStatus::Paid,
        InvoiceStatus::Pending,
        InvoiceStatus::PartiallyPaid,
        InvoiceStatus::Voided,
        InvoiceStatus::Closed,
    ] {
        let mut invoice = cash_sale_invoice();
        invoice.status = status;
        let payload = build_a4_invoice_data(
            &invoice,
            "DUPLICATE COPY",
            true,
            invoice.amount_received_cents.unwrap_or(0),
        );
        assert_matches_schema("a4-invoice", &payload);
    }
}

/// The nastiest drift class: fields that serialize to JSON `null`
/// (`vatRatePercent` when the shop snapshot carries no VAT rate) or come
/// from an empty snapshot entirely — a minimal shop profile must still
/// produce a schema-valid payload for both receipt layouts.
#[test]
fn split_payment_credit_invoice_with_empty_shop_matches_schemas() {
    let mut invoice = cash_sale_invoice();
    invoice.status = InvoiceStatus::PartiallyPaid;
    invoice.payment_method = "split".to_string();
    invoice.is_credit = true;
    invoice.due_date = Some("2026-09-06".to_string());
    invoice.amount_received_cents = Some(50000);
    invoice.change_due_cents = None;
    invoice.split_payments = Some(vec![
        serde_json::from_value(json!({
            "method": "cash", "amountCents": 50000
        }))
        .unwrap(),
        serde_json::from_value(json!({
            "method": "card", "amountCents": 54500, "cardLast4": "4242"
        }))
        .unwrap(),
    ]);
    // No VAT rate key → builder emits `vatRatePercent: null`; no shop
    // fields at all beyond what `shop_field_or_empty` can fall back on.
    invoice.shop_profile_snapshot = json!({});

    let thermal =
        build_thermal_receipt_data(&invoice, 58, invoice.amount_received_cents.unwrap_or(0));
    assert_matches_schema("thermal-receipt", &thermal);

    let a4 = build_a4_invoice_data(
        &invoice,
        "ORIGINAL — CUSTOMER COPY",
        false,
        invoice.amount_received_cents.unwrap_or(0),
    );
    assert_matches_schema("a4-invoice", &a4);
}

fn base_credit_note() -> CreditNote {
    serde_json::from_value(json!({
        "id": "64b0000000000000000000b1",
        "key": "cn_test",
        "creditNoteNumber": "CN-000001",
        "invoiceKey": "inv_testcash",
        "invoiceNumber": "INV-000001",
        "noReceipt": false,
        "customerKey": "cust_test",
        "customerNameSnapshot": "Kasun Silva",
        "cashierId": "usr_admin",
        "cashierNameSnapshot": "Default Admin",
        "returnedItems": [
            {
                "productKey": "prod_test",
                "name": "Screen Protector - iPhone 14",
                "sku": "ACC-SP-014",
                "quantity": 1,
                "unitPriceCents": 15000,
                "totalCents": 15000,
                "reason": "defective",
                "condition": "damaged",
                "disposition": "write_off_scrap"
            }
        ],
        "returnSubtotalCents": 15000,
        "exchangeSubtotalCents": 0,
        "netRefundCents": 15000,
        "refundCashCents": 10000,
        "balanceReductionCents": 5000,
        "refundBreakdown": [
            {"method": "cash", "amountCents": 10000}
        ],
        "status": "resolved",
        "isManagerOverride": false,
        "shopProfileSnapshot": {},
        "createdAt": "2026-08-23T10:00:00Z",
        "updatedAt": "2026-08-23T10:00:00Z"
    }))
    .expect("credit note fixture deserializes")
}

#[test]
fn credit_note_payloads_match_published_schema() {
    let invoice = cash_sale_invoice();

    let linked = build_credit_note_data(&base_credit_note(), Some(&invoice));
    assert_matches_schema("credit-note", &linked);

    // No-receipt variant: no shop profile exists, so every shop field
    // falls back to empty — must still satisfy the required strings.
    let mut no_receipt = base_credit_note();
    no_receipt.no_receipt = true;
    no_receipt.invoice_key = None;
    no_receipt.invoice_number = None;
    let payload = build_credit_note_data(&no_receipt, None);
    assert_matches_schema("credit-note", &payload);
}

fn quotation(status: &str, discount_cents: i64) -> crate::domain::quotations::Quotation {
    use crate::domain::quotations::{Quotation, QuotationItem};
    Quotation {
        id: "64b0000000000000000000b1".to_string(),
        key: "quo_test".to_string(),
        quotation_number: "QUO-0001".to_string(),
        status: status.to_string(),
        is_expired: false,
        has_out_of_stock_items: false,
        fulfilled_by_invoice_key: None,
        fulfilled_at: None,
        customer_key: None,
        customer_name: "Nimal Perera".to_string(),
        customer_phone: Some("077 123 4567".to_string()),
        customer_address: None,
        items: vec![QuotationItem {
            product_key: None,
            name: "Screen replacement".to_string(),
            sku: None,
            unit_price_cents: 10_000,
            quantity: 2,
            discount_cents: 0,
            total_cents: 20_000,
        }],
        subtotal_cents: 20_000,
        discount_type: "fixed".to_string(),
        discount_value: discount_cents as f64,
        discount_cents,
        total_cents: 20_000 - discount_cents,
        valid_until: "2030-01-31".to_string(),
        notes: None,
        terms: Some("50% advance.".to_string()),
        prepared_by: Some("Kamal".to_string()),
        created_at: "2026-10-13T09:00:00Z".parse().unwrap(),
        updated_at: "2026-10-13T09:00:00Z".parse().unwrap(),
    }
}

#[test]
fn quotation_payload_matches_published_schema() {
    let shop = cash_sale_invoice().shop_profile_snapshot;
    assert_matches_schema(
        "quotation",
        &build_quotation_data(&quotation("active", 0), &shop),
    );
    // Cancelled, discounted, and printed for a shop that never filled in
    // its profile (every shop field empty): still satisfies the contract.
    assert_matches_schema(
        "quotation",
        &build_quotation_data(&quotation("cancelled", 500), &json!({})),
    );
}

#[test]
fn quotation_payload_carries_dates_cancellation_and_words() {
    let shop = json!({ "tradingName": "TechFix", "logoBase64": "data:image/png;base64,AAAA" });
    let active = build_quotation_data(&quotation("active", 0), &shop);
    assert_eq!(active["quotationNumber"], "QUO-0001");
    assert_eq!(active["formattedDate"], "13 Oct 2026");
    assert_eq!(active["validUntil"], "31 Jan 2030");
    assert_eq!(active["isCancelled"], false);
    assert_eq!(active["logoUrl"], "data:image/png;base64,AAAA");
    assert_eq!(active["shopTradingName"], "TechFix");
    assert_eq!(active["totalCents"], 20_000);
    assert_eq!(
        active["amountInWords"],
        "Sri Lankan Rupees Two Hundred Only"
    );
    // A quotation is an offer: none of the invoice-only fields leak in.
    for invoice_only in ["cashierName", "paymentMethod", "balanceDueCents", "status"] {
        assert!(active.get(invoice_only).is_none(), "{invoice_only}");
    }

    let cancelled = build_quotation_data(&quotation("cancelled", 0), &shop);
    assert_eq!(cancelled["isCancelled"], true);
}

fn base_z_report() -> ZReportResponse {
    use crate::domain::sessions::{
        CashDirection, CashMovement, CashMovementType, RegisterSession, SessionStatus,
        ZReportCashReconciliation, ZReportPaymentMethodSummary, ZReportResponse,
    };
    ZReportResponse {
        session: RegisterSession {
            id: "64b0000000000000000000c1".to_string(),
            key: "ses_test".to_string(),
            session_number: "SES-000042".to_string(),
            terminal_id: "TERM-01".to_string(),
            cashier_id: "usr_admin".to_string(),
            cashier_name_snapshot: "Kasun Perera".to_string(),
            status: SessionStatus::Closed,
            opened_at: "2026-10-21T08:30:00Z".parse().unwrap(),
            opening_float_cents: 200_000,
            opening_denominations: None,
            opening_note: Some("Morning opening".to_string()),
            opening_discrepancy_cents: 0,
            opening_discrepancy_reason: None,
            opening_approved_by_id: None,
            opening_approved_by_name: None,
            closed_at: Some("2026-10-21T18:00:00Z".parse().unwrap()),
            closing_cashier_id: Some("usr_admin".to_string()),
            closing_cashier_name_snapshot: Some("Kasun Perera".to_string()),
            expected_cash_cents: Some(520_000),
            counted_cash_cents: Some(520_000),
            cash_variance_cents: Some(0),
            closing_float_cents: Some(200_000),
            cash_drop_to_safe_cents: Some(320_000),
            closing_denominations: None,
            closing_note: Some("Reconciled perfectly".to_string()),
            variance_reason: None,
            variance_approved_by_id: None,
            variance_approved_by_name: None,
            reopened_at: None,
            reopened_by_id: None,
            reopened_by_name: None,
            reopened_reason: None,
            has_late_movements: false,
            total_cash_sales_cents: 1_150_000,
            total_card_sales_cents: 550_000,
            total_online_sales_cents: 150_000,
            total_credit_sales_cents: 0,
            total_cash_refunds_cents: 50_000,
            total_pay_in_cents: 50_000,
            total_pay_out_cents: 30_000,
            total_owner_withdrawals_cents: 100_000,
            total_cash_drops_cents: 700_000,
            created_at: "2026-10-21T08:30:00Z".parse().unwrap(),
            updated_at: "2026-10-21T18:00:00Z".parse().unwrap(),
            version: 1,
        },
        gross_sales_cents: 1_850_000,
        returns_cents: 50_000,
        net_sales_cents: 1_800_000,
        discount_cents: 25_000,
        total_invoices_count: 24,
        payment_methods: vec![
            ZReportPaymentMethodSummary {
                method: "cash".to_string(),
                count: 14,
                total_cents: 1_150_000,
            },
            ZReportPaymentMethodSummary {
                method: "card".to_string(),
                count: 8,
                total_cents: 550_000,
            },
        ],
        cash_movements: vec![CashMovement {
            id: "64b0000000000000000000d1".to_string(),
            key: "cm_test".to_string(),
            movement_number: "CM-000101".to_string(),
            movement_type: CashMovementType::AddCash,
            direction: CashDirection::In,
            amount_cents: 50_000,
            session_key: Some("ses_test".to_string()),
            terminal_id: "TERM-01".to_string(),
            category_key: "ccat_add_cash_test".to_string(),
            category_name: "Change float top-up".to_string(),
            custom_label: None,
            payee_key: None,
            payee_name: Some("Main Safe".to_string()),
            period_start: None,
            period_end: None,
            reference_no: None,
            note: None,
            approval_reason: None,
            reverses_key: None,
            reversed_by_key: None,
            undo_until: None,
            recorded_by_user_id: "usr_admin".to_string(),
            recorded_by_name_snapshot: "Kasun Perera".to_string(),
            approved_by_user_id: None,
            approved_by_name_snapshot: None,
            recorded_late: false,
            original_variance_session_key: None,
            receipt_key: None,
            created_at: "2026-10-21T10:15:00Z".parse().unwrap(),
            updated_at: "2026-10-21T10:15:00Z".parse().unwrap(),
            version: 1,
        }],
        movement_summary: vec![],
        cash_reconciliation: ZReportCashReconciliation {
            opening_float_cents: 200_000,
            cash_sales_cents: 1_150_000,
            pay_in_cents: 50_000,
            pay_out_cents: 30_000,
            cash_refunds_cents: 50_000,
            owner_withdrawals_cents: 100_000,
            cash_drops_cents: 700_000,
            expected_cash_cents: 520_000,
            counted_cash_cents: 520_000,
            variance_cents: 0,
            closing_float_cents: 200_000,
            safe_deposit_cents: 320_000,
        },
        generated_at: "2026-10-21T18:02:00Z".parse().unwrap(),
    }
}

#[test]
fn z_report_payload_matches_published_schema() {
    let shop = cash_sale_invoice().shop_profile_snapshot;
    assert_matches_schema("z-report", &build_z_report_data(&base_z_report(), &shop));

    // Open session with minimal/empty shop profile: must still satisfy contract
    let mut open_report = base_z_report();
    open_report.session.status = crate::domain::sessions::SessionStatus::Open;
    open_report.session.closed_at = None;
    assert_matches_schema("z-report", &build_z_report_data(&open_report, &json!({})));
}

#[test]
fn z_report_payload_carries_exact_reconciliation_math() {
    let shop = json!({ "tradingName": "TechFix", "logoBase64": "data:image/png;base64,AAAA" });
    let payload = build_z_report_data(&base_z_report(), &shop);
    assert_eq!(payload["sessionNumber"], "SES-000042");
    assert_eq!(payload["terminalId"], "TERM-01");
    assert_eq!(payload["cashierName"], "Kasun Perera");
    assert_eq!(payload["status"], "closed");
    assert_eq!(payload["grossSalesCents"], 1_850_000);
    assert_eq!(payload["returnsCents"], 50_000);
    assert_eq!(payload["netSalesCents"], 1_800_000);
    assert_eq!(payload["discountCents"], 25_000);
    assert_eq!(payload["totalInvoicesCount"], 24);
    assert_eq!(payload["cashReconciliation"]["expectedCashCents"], 520_000);
    assert_eq!(payload["cashReconciliation"]["countedCashCents"], 520_000);
    assert_eq!(payload["cashReconciliation"]["varianceCents"], 0);
}
