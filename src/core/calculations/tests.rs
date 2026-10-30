use super::compute_order_discount::compute_order_discount;
use super::compute_sale_totals::compute_sale_totals;
use super::validate_split_payments::validate_split_payments;

use crate::domain::billing::{InvoiceItem, PricingAdjustments, SplitPayment};

use super::*;

#[test]
fn test_compute_line_total() {
    assert_eq!(compute_line_total(490, 2, 0), 980);
    assert_eq!(compute_line_total(650, 1, 65), 585);
    assert_eq!(compute_line_total(500, 1, 800), 0);
}

#[test]
fn test_compute_order_discount_percentage_half_up() {
    let adj = PricingAdjustments {
        discount_type: "percentage".to_string(),
        discount_value: 10.0,
    };
    let (dtype, dval, dcents) = compute_order_discount(1140, Some(&adj));
    assert_eq!(dtype, "percentage");
    assert_eq!(dval, 10.0);
    assert_eq!(dcents, 114);
}

#[test]
fn test_compute_order_discount_fixed() {
    let adj = PricingAdjustments {
        discount_type: "fixed".to_string(),
        discount_value: 140.0,
    };
    let (_, _, dcents) = compute_order_discount(1140, Some(&adj));
    assert_eq!(dcents, 140);
}

#[test]
fn test_compute_sale_totals() {
    let items = vec![
        InvoiceItem {
            product_key: None,
            name: "Item 1".to_string(),
            sku: None,
            unit_price_cents: 490,
            quantity: 1,
            discount_cents: 0,
            total_cents: 490,
            unit_cost_cents: None,
            source_type: "retail".to_string(),
            source_ticket_key: None,
            source_ticket_number: None,
            assigned_employee_name: None,
            returned_quantity: 0,
            serial_numbers: vec![],
            batch_allocations: vec![],
        },
        InvoiceItem {
            product_key: None,
            name: "Item 2".to_string(),
            sku: None,
            unit_price_cents: 650,
            quantity: 1,
            discount_cents: 0,
            total_cents: 650,
            unit_cost_cents: None,
            source_type: "retail".to_string(),
            source_ticket_key: None,
            source_ticket_number: None,
            assigned_employee_name: None,
            returned_quantity: 0,
            serial_numbers: vec![],
            batch_allocations: vec![],
        },
    ];

    let (subtotal, _, _, discount, total) = compute_sale_totals(&items, None);
    assert_eq!(subtotal, 1140);
    assert_eq!(discount, 0);
    assert_eq!(total, 1140);
}

#[test]
fn test_compute_change_due() {
    assert_eq!(compute_change_due(Some(2000), 1140), Some(860));
    assert_eq!(compute_change_due(Some(1000), 1140), Some(0));
    assert_eq!(compute_change_due(None, 1140), None);
}

#[test]
fn test_validate_split_payments() {
    let legs = vec![
        SplitPayment {
            method: "cash".to_string(),
            amount_cents: 500,
            card_last4: None,
            reference: None,
        },
        SplitPayment {
            method: "card".to_string(),
            amount_cents: 500,
            card_last4: None,
            reference: None,
        },
    ];
    assert!(validate_split_payments(1000, &legs).is_ok());
    assert!(validate_split_payments(900, &legs).is_err());
    assert!(validate_split_payments(1000, &[]).is_err());
}
