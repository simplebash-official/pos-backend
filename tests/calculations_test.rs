use jana2u_pos_backend::{
    core::calculations::{
        compute_change_due, compute_line_total, compute_order_discount, compute_sale_totals,
        validate_split_payments,
    },
    domain::billing::{InvoiceItem, PricingAdjustments, SplitPayment},
};

#[test]
fn test_line_total_calculations() {
    assert_eq!(compute_line_total(490, 2, 0), 980);
    assert_eq!(compute_line_total(650, 1, 50), 600);
    assert_eq!(compute_line_total(1000, 3, 500), 2500);
    // Discount exceeds gross
    assert_eq!(compute_line_total(500, 1, 1000), 0);
    // Negative protection
    assert_eq!(compute_line_total(-500, 1, 0), 0);
}

#[test]
fn test_order_discount_half_up_rounding() {
    // Exact scenario reported by user: items 490 and 650 -> subtotal 1140
    let adj_10_pct = PricingAdjustments {
        discount_type: "percentage".to_string(),
        discount_value: 10.0,
    };
    let (dtype, dval, dcents) = compute_order_discount(1140, Some(&adj_10_pct));
    assert_eq!(dtype, "percentage");
    assert_eq!(dval, 10.0);
    assert_eq!(dcents, 114);

    // 7.5% on 1140 -> 85.5 -> rounds to 86
    let adj_7_5_pct = PricingAdjustments {
        discount_type: "percentage".to_string(),
        discount_value: 7.5,
    };
    let (_, _, dcents_7_5) = compute_order_discount(1140, Some(&adj_7_5_pct));
    assert_eq!(dcents_7_5, 86);
}

#[test]
fn test_order_discount_fixed_and_clamping() {
    let adj_fixed = PricingAdjustments {
        discount_type: "fixed".to_string(),
        discount_value: 500.0,
    };
    let (_, _, dcents) = compute_order_discount(1140, Some(&adj_fixed));
    assert_eq!(dcents, 500);

    // Fixed discount exceeds subtotal -> clamped to subtotal
    let adj_overflow = PricingAdjustments {
        discount_type: "fixed".to_string(),
        discount_value: 2000.0,
    };
    let (_, _, dcents_clamped) = compute_order_discount(1140, Some(&adj_overflow));
    assert_eq!(dcents_clamped, 1140);
}

#[test]
fn test_sale_totals_consistency() {
    let items = vec![
        InvoiceItem {
            product_key: Some("prod_1".to_string()),
            name: "Item 1 (Rs. 4.90)".to_string(),
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
        },
        InvoiceItem {
            product_key: Some("prod_2".to_string()),
            name: "Item 2 (Rs. 6.50)".to_string(),
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
        },
    ];

    let (subtotal, dtype, dval, discount, total) = compute_sale_totals(&items, None);
    assert_eq!(subtotal, 1140);
    assert_eq!(dtype, "fixed");
    assert_eq!(dval, 0.0);
    assert_eq!(discount, 0);
    assert_eq!(total, 1140);

    // Change due on Rs. 20.00 cash
    let change = compute_change_due(Some(2000), total);
    assert_eq!(change, Some(860));
}

#[test]
fn test_validate_split_payments() {
    let legs = vec![
        SplitPayment {
            method: "cash".to_string(),
            amount_cents: 640,
            card_last4: None,
            reference: None,
        },
        SplitPayment {
            method: "card".to_string(),
            amount_cents: 500,
            card_last4: Some("4321".to_string()),
            reference: None,
        },
    ];
    assert!(validate_split_payments(1140, &legs).is_ok());

    let invalid_legs = vec![
        SplitPayment {
            method: "cash".to_string(),
            amount_cents: 1000,
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
    // 1500 > 1140 -> error
    assert!(validate_split_payments(1140, &invalid_legs).is_err());
}
