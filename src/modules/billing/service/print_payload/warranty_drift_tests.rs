//! The warranty parts of printed documents against the real published schemas: the notes
//! under invoice lines (every A4 invoice and receipt design) and the warranty card.

use super::schema_drift_tests::{assert_matches_schema, cash_sale_invoice};
use super::warranty_print::{add_line_warranties, build_warranty_card_data};
use super::{build_a4_invoice_data, build_thermal_receipt_data};
use crate::domain::warranties::{Warranty, WarrantyCardPaper};
use serde_json::{Value, json};

/// A stored warranty as the API returns it; `overrides` replace fields of the sold-serial default.
fn warranty(overrides: Value) -> Warranty {
    let mut value = json!({
        "id": "wty_1",
        "key": "wty_1",
        "warrantyNumber": "WTY-000001",
        "sourceType": "product_sale",
        "invoiceKey": "inv_testcash",
        "invoiceNumber": "INV-000001",
        "lineIndex": 0,
        "productKey": "prod_test",
        "itemName": "Screen Protector - iPhone 14",
        "serialNumber": "SN-A",
        "quantity": 1,
        "customerName": "Kasun Silva",
        "customerPhone": "077 123 4567",
        "coverage": {"kind": "limited", "value": 12, "unit": "months"},
        "warrantyType": "shop",
        "terms": "Covers manufacturing faults.",
        "unitPriceCents": 15000,
        "startsOn": "2026-08-23",
        "endsOn": "2027-08-22",
        "status": "active",
        "claimCount": 0,
        "state": "active",
        "daysRemaining": 365,
        "version": 1,
        "createdAt": "2026-08-23T09:00:00Z",
        "updatedAt": "2026-08-23T09:00:00Z"
    });
    for (field, overridden) in overrides.as_object().expect("overrides are an object") {
        value[field] = overridden.clone();
    }
    serde_json::from_value(value).expect("warranty fixture deserializes")
}

/// Two serials on the first line and a lifetime repair warranty on the second.
fn invoice_warranties() -> Vec<Warranty> {
    vec![
        warranty(json!({})),
        warranty(json!({"key": "wty_2", "warrantyNumber": "WTY-000002", "serialNumber": "SN-B"})),
        warranty(json!({
            "key": "wty_3",
            "warrantyNumber": "WTY-000003",
            "sourceType": "repair",
            "lineIndex": 1,
            "itemName": "Screen replacement (iPhone 13)",
            "coverage": {"kind": "lifetime"},
            "terms": null,
            "endsOn": null,
            "daysRemaining": null
        })),
    ]
}

#[test]
fn line_warranties_print_under_their_lines_and_match_every_invoice_schema() {
    let invoice = cash_sale_invoice();
    let mut a4 = build_a4_invoice_data(&invoice, "ORIGINAL", false, 104_500);
    add_line_warranties(&mut a4, &invoice_warranties()).expect("notes are added");
    let mut receipt = build_thermal_receipt_data(&invoice, 58, 104_500);
    add_line_warranties(&mut receipt, &invoice_warranties()).expect("notes are added");

    let first = &a4["items"][0]["warranties"];
    assert_eq!(first.as_array().expect("an array").len(), 2);
    assert_eq!(
        first[0],
        json!({
            "number": "WTY-000001",
            "subject": "SN-A",
            "coverageLabel": "12 months",
            "validUntil": "22 Aug 2027",
            "terms": "Covers manufacturing faults."
        })
    );
    assert_eq!(first[1]["subject"], "SN-B");
    let repair = &a4["items"][1]["warranties"][0];
    assert_eq!(repair["subject"], "Screen replacement (iPhone 13)");
    assert_eq!(repair["coverageLabel"], "Lifetime");
    assert_eq!(repair["validUntil"], Value::Null);
    assert_eq!(repair["terms"], "");

    assert_matches_schema("a4-invoice", &a4);
    assert_matches_schema("thermal-receipt", &receipt);
}

#[test]
fn a_line_without_a_warranty_carries_no_warranties_field() {
    let invoice = cash_sale_invoice();
    let mut a4 = build_a4_invoice_data(&invoice, "ORIGINAL", false, 104_500);
    add_line_warranties(&mut a4, &[warranty(json!({"lineIndex": 1}))]).expect("notes are added");
    assert!(a4["items"][0].get("warranties").is_none());
    assert_eq!(a4["items"][1]["warranties"][0]["number"], "WTY-000001");
}

#[test]
fn a_warranty_on_a_line_the_invoice_lacks_is_an_error() {
    let invoice = cash_sale_invoice();
    let mut a4 = build_a4_invoice_data(&invoice, "ORIGINAL", false, 104_500);
    let error = add_line_warranties(&mut a4, &[warranty(json!({"lineIndex": 7}))])
        .expect_err("line 7 does not exist");
    assert!(error.to_string().contains("line 7"), "{error}");
}

#[test]
fn warranty_card_matches_its_schema_on_every_paper() {
    let invoice = cash_sale_invoice();
    for paper in [
        WarrantyCardPaper::A5,
        WarrantyCardPaper::Thermal58,
        WarrantyCardPaper::Thermal80,
    ] {
        let card = build_warranty_card_data(&warranty(json!({})), &invoice, paper, None)
            .expect("card data builds");
        assert_eq!(card["paper"], paper.as_str());
        assert_matches_schema("warranty-card", &card);
    }
}

#[test]
fn warranty_card_carries_period_void_replacement_and_policy() {
    let mut invoice = cash_sale_invoice();
    invoice.warranty_terms_snapshot = Some("Bring the invoice with the item.".to_string());
    let card =
        build_warranty_card_data(&warranty(json!({})), &invoice, WarrantyCardPaper::A5, None)
            .expect("card data builds");
    assert_eq!(card["warrantyNumber"], "WTY-000001");
    assert_eq!(card["coverageLabel"], "12 months");
    assert_eq!(card["warrantyTypeLabel"], "Shop warranty");
    assert_eq!(card["startsOn"], "23 Aug 2026");
    assert_eq!(card["validUntil"], "22 Aug 2027");
    assert_eq!(card["isVoid"], false);
    assert_eq!(card["replacesWarrantyNumber"], "");
    assert_eq!(card["purchaseDate"], "23 Aug 2026");
    assert_eq!(card["generalPolicy"], "Bring the invoice with the item.");

    // A lifetime replacement that was later voided; the shop's default text is the policy.
    invoice.warranty_terms_snapshot = None;
    invoice.shop_profile_snapshot["defaultWarrantyText"] = json!("Shop policy.");
    let replaced = warranty(json!({
        "coverage": {"kind": "lifetime"},
        "endsOn": null,
        "status": "void",
        "state": "void",
        "warrantyType": "both",
        "serialNumber": null,
        "customerName": null,
        "customerPhone": null
    }));
    let card = build_warranty_card_data(
        &replaced,
        &invoice,
        WarrantyCardPaper::Thermal58,
        Some("WTY-000009"),
    )
    .expect("card data builds");
    assert_eq!(card["validUntil"], Value::Null);
    assert_eq!(card["coverageLabel"], "Lifetime");
    assert_eq!(card["isVoid"], true);
    assert_eq!(card["replacesWarrantyNumber"], "WTY-000009");
    assert_eq!(card["serialNumber"], "");
    assert_eq!(card["customerName"], "");
    assert_eq!(card["generalPolicy"], "Shop policy.");
    assert_eq!(card["warrantyTypeLabel"], "Shop and manufacturer warranty");
    assert_matches_schema("warranty-card", &card);
}
