use pos_calc_core::{
    cash::{self, CalcCashCountRequest},
    checkout::{
        CalcCartLine, CalcCheckoutPayment, CalcCheckoutRequest, CalcDiscount, CalcDiscountType,
        CalcPaymentMethod, CalcSource, compute_checkout,
    },
    earnings::{self, CalcEarningsRequest, SplitKind},
    receipt::{
        Allocation, BilledBy, CalcCharge, CalcChargesPayment, CalcLine, CalcPayment,
        CalcReceiptRequest, ChargeKind, CostMode, PaymentMode, PaymentSource, PricingMode,
        compute as compute_receipt,
    },
};
use std::collections::HashMap;

#[test]
fn test_checkout_subtotal_and_discounts_vector() {
    // 1. Checkout Subtotal & Discounts:
    // - Item A: 2x Rs 450.00 = Rs 900.00 (90,000 cents)
    // - Item B: 1x Rs 1,200.00 with line discount 10% = Rs 1,080.00 (108,000 cents)
    // - Cart Subtotal = Rs 1,980.00 (198,000 cents)
    // - Order Discount: 5% = Rs 99.00 (9,900 cents)
    // - Net Total = Rs 1,881.00 (188,100 cents)
    // - Tendered: Rs 2,000.00 (200,000 cents) -> Change Due: Rs 119.00 (11,900 cents)
    let req = CalcCheckoutRequest {
        items: vec![
            CalcCartLine {
                key: "item-a".into(),
                unit_price_cents: 45_000,
                quantity: 2,
                discount: None,
                source_type: CalcSource::Retail,
            },
            CalcCartLine {
                key: "item-b".into(),
                unit_price_cents: 120_000,
                quantity: 1,
                discount: Some(CalcDiscount {
                    kind: CalcDiscountType::Percentage,
                    value: 10.0,
                }),
                source_type: CalcSource::Retail,
            },
        ],
        order_discount: Some(CalcDiscount {
            kind: CalcDiscountType::Percentage,
            value: 5.0,
        }),
        payment: CalcCheckoutPayment {
            method: CalcPaymentMethod::Cash,
            tendered_cents: 200_000,
            split_legs_cents: vec![],
            customer_balance_cents: 0,
            is_credit: false,
            credit_deposit_cents: 0,
        },
    };

    let reply = compute_checkout(&req).expect("checkout calculation should succeed");

    assert_eq!(reply.items[0].gross_cents, 90_000);
    assert_eq!(reply.items[0].discount_cents, 0);
    assert_eq!(reply.items[0].total_cents, 90_000);

    assert_eq!(reply.items[1].gross_cents, 120_000);
    assert_eq!(reply.items[1].discount_cents, 12_000);
    assert_eq!(reply.items[1].total_cents, 108_000);

    assert_eq!(reply.subtotal_cents, 198_000);
    assert_eq!(reply.order_discount_cents, 9_900);
    assert_eq!(reply.total_cents, 188_100);
    assert_eq!(reply.payment.change_due_cents, 11_900);
    assert_eq!(reply.payment.short_by_cents, 0);
    assert!(reply.payment.is_fully_paid);
}

#[test]
fn test_multi_tender_split_payment() {
    // Multi-tender split payments (Cash Rs 5,000 + Card Rs 12,350 against Rs 17,350 total).
    let req = CalcCheckoutRequest {
        items: vec![CalcCartLine {
            key: "item-1".into(),
            unit_price_cents: 1_735_000,
            quantity: 1,
            discount: None,
            source_type: CalcSource::Retail,
        }],
        order_discount: None,
        payment: CalcCheckoutPayment {
            method: CalcPaymentMethod::Split,
            tendered_cents: 0,
            split_legs_cents: vec![500_000, 1_235_000],
            customer_balance_cents: 0,
            is_credit: false,
            credit_deposit_cents: 0,
        },
    };

    let reply = compute_checkout(&req).expect("split checkout should succeed");
    assert_eq!(reply.total_cents, 1_735_000);
    assert_eq!(reply.payment.split_allocated_cents, 1_735_000);
    assert_eq!(reply.payment.split_remaining_cents, 0);
    assert!(reply.payment.is_split_valid);
    assert!(reply.payment.is_fully_paid);
}

#[test]
fn test_landed_cost_largest_remainder_vector() {
    // 2. Landed Cost Spreading (Largest Remainder Rule):
    // - Stock Line 1: 10 units @ Rs 500 = Rs 5,000 (500,000 cents)
    // - Stock Line 2: 20 units @ Rs 250 = Rs 5,000 (500,000 cents)
    // - Total Cost: Rs 10,000 (1,000,000 cents)
    // - Delivery Charge: Rs 333 (33,300 cents) allocated by Value
    // - Distribution: Line 1 gets Rs 166.50 -> rounded to Rs 167 (16,650 cents half/tie goes to earlier line -> 16,650 cents, but wait: 33,300 cents is even!)
    // Wait, let's test an odd number: Rs 333.33 = 33,333 cents!
    // 33,333 cents over 500,000 : 500,000 (1:1 equal weights):
    // Line 1 gets 16,667 cents, Line 2 gets 16,666 cents. Sum = 33,333 cents!
    let req = CalcReceiptRequest {
        supplier_key: Some("sup_1".into()),
        reference_no: "REC-001".into(),
        date: "2026-10-28".into(),
        lines: vec![
            CalcLine {
                product_key: Some("prod_1".into()),
                serialized: false,
                quantity: Some(10),
                cost_mode: CostMode::Unit,
                cost_cents: 50_000, // Rs 500
                pricing_mode: PricingMode::Price,
                selling_price_cents: 75_000,
                margin_tenths: None,
                step_cents: None,
                serial_numbers: vec![],
            },
            CalcLine {
                product_key: Some("prod_2".into()),
                serialized: false,
                quantity: Some(20),
                cost_mode: CostMode::Unit,
                cost_cents: 25_000, // Rs 250
                pricing_mode: PricingMode::Price,
                selling_price_cents: 40_000,
                margin_tenths: None,
                step_cents: None,
                serial_numbers: vec![],
            },
        ],
        charges: vec![CalcCharge {
            kind: Some(ChargeKind::Shipping),
            amount_cents: 33_333,
            billed_by: Some(BilledBy::Supplier),
            allocation: Some(Allocation::Value),
            description: "Delivery".into(),
        }],
        charges_payment: CalcChargesPayment {
            source: Some(PaymentSource::Drawer),
            paid_on: "2026-10-28".into(),
        },
        payment: CalcPayment {
            mode: PaymentMode::Full,
            source: Some(PaymentSource::Bank),
            part_cents: 0,
            paid_on: "2026-10-28".into(),
            category_key: Some("cat_1".into()),
        },
    };

    let reply = compute_receipt(&req).expect("receipt calculation should succeed");
    assert_eq!(reply.lines[0].goods_cents, 500_000);
    assert_eq!(reply.lines[1].goods_cents, 500_000);
    // Line 1 gets 16,667 cents, Line 2 gets 16,666 cents
    assert_eq!(reply.lines[0].landed_cents, 516_667);
    assert_eq!(reply.lines[1].landed_cents, 516_666);
    assert_eq!(
        reply.lines[0].landed_cents + reply.lines[1].landed_cents,
        1_000_000 + 33_333
    );
}

#[test]
fn test_technician_job_commission_vector() {
    // 3. Technician Job Commission:
    // - Repair Estimated Price: Rs 6,500.00 (650,000 cents)
    // - Parts/Material Cost: Rs 2,150.00 (215,000 cents)
    // - Net Profit: Rs 4,350.00 (435,000 cents)
    // - 25% Split Commission: Rs 1,087.50 -> 108,750 cents
    let req_pct = CalcEarningsRequest {
        price_cents: Some(650_000),
        material_cost_cents: 215_000,
        split_type: Some(SplitKind::Percentage),
        split_value: 25.0,
    };
    let reply_pct = earnings::compute(&req_pct).expect("percentage commission should succeed");
    assert_eq!(reply_pct.profit_cents, 435_000);
    assert_eq!(reply_pct.commission_cents, 108_750);
    assert_eq!(reply_pct.shop_keeps_cents, 326_250);
    assert!(!reply_pct.capped);

    // - Fixed Rs 5,000 Split Commission (500,000 cents): Capped at Net Profit Rs 4,350.00 (435,000 cents)
    let req_fixed = CalcEarningsRequest {
        price_cents: Some(650_000),
        material_cost_cents: 215_000,
        split_type: Some(SplitKind::Fixed),
        split_value: 500_000.0,
    };
    let reply_fixed = earnings::compute(&req_fixed).expect("fixed commission should succeed");
    assert_eq!(reply_fixed.profit_cents, 435_000);
    assert_eq!(reply_fixed.commission_cents, 435_000);
    assert_eq!(reply_fixed.shop_keeps_cents, 0);
    assert!(reply_fixed.capped);
}

#[test]
fn test_rupee_note_counting() {
    let mut counts = HashMap::new();
    counts.insert("5000".into(), 2); // 10,000
    counts.insert("1000".into(), 5); // 5,000
    counts.insert("500".into(), 4); // 2,000
    counts.insert("100".into(), 10); // 1,000
    counts.insert("50".into(), 6); // 300
    counts.insert("20".into(), 5); // 100
    counts.insert("10".into(), 10); // 100
    // Total = Rs 18,500 = 1,850,000 cents

    let req = CalcCashCountRequest { counts };
    let reply = cash::compute_count(&req).expect("cash count should succeed");
    assert_eq!(reply.total_cents, 1_850_000);
    assert_eq!(reply.lines.len(), 10);
}
