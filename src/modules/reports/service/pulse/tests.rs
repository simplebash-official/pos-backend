use super::compute::compute;

use super::service_pulse::PulseInput;

use crate::domain::employees::Employee;
use crate::domain::inventory::Product;

use crate::domain::quotations::Quotation;
use crate::domain::repairs::Repair;

use serde_json::{Value, json};

const NOW: &str = "2026-10-26T12:00:00Z";

fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap_or_else(|e| panic!("{e}"))
}

fn repair(id: &str, status: &str, tech: Option<&str>, created: &str, cost: i64) -> Repair {
    from(json!({
        "id": id, "key": id, "ticketNumber": format!("REP-{id}"), "customerName": "Asha",
        "customerPhone": "0771234567", "deviceModel": "Phone", "issueDescription": "x",
        "isOverdue": false, "status": status, "estimatedCostCents": cost,
        "materialCostCents": 500, "assignedEmployeeId": tech, "createdAt": created,
        "updatedAt": created, "version": 1
    }))
}

fn employee(id: &str, role: &str) -> Employee {
    from(json!({
        "id": id, "key": format!("emp_{id}"), "name": format!("Name {id}"), "phone": "0770000000", "role": role,
        "defaultSplitType": "percentage", "defaultSplitValue": 30.0, "status": "active",
        "createdAt": NOW, "updatedAt": NOW, "version": 1
    }))
}

fn input<'a>(repairs: &'a [Repair], employees: &'a [Employee]) -> PulseInput<'a> {
    PulseInput {
        now: NOW.parse().unwrap(),
        repairs,
        print_jobs: &[],
        products: &[],
        employees,
        today_invoices: &[],
        recent_invoices: &[],
        active_quotations: &[],
    }
}

#[test]
fn repairs_are_grouped_into_the_pipeline_with_rounded_shares() {
    let repairs = [
        repair("a", "received", None, NOW, 1_000),
        repair("b", "received", None, NOW, 1_000),
        repair("c", "in_repair", None, NOW, 1_000),
    ];
    let pulse = compute(&input(&repairs, &[]));
    let counts: Vec<(i64, i64)> = pulse
        .repair_pipeline
        .iter()
        .map(|s| (s.count, s.percent))
        .collect();
    assert_eq!(counts, [(2, 67), (0, 0), (1, 33), (0, 0)]);
    assert_eq!(pulse.active_repairs_count, 3);
}

#[test]
fn a_ready_repair_waiting_three_days_is_critical_and_counts_towards_the_waiting_value() {
    let repairs = [
        repair("a", "ready", None, "2026-10-23T11:00:00Z", 10_000),
        repair("b", "ready", None, "2026-10-26T08:00:00Z", 2_000),
    ];
    let pulse = compute(&input(&repairs, &[]));
    assert_eq!(pulse.uncollected_ready_value_cents, 10_500 + 2_500);
    assert_eq!(pulse.ready_repairs[0].severity, "critical");
    assert_eq!(pulse.ready_repairs[0].days_waiting, 3);
    assert_eq!(pulse.ready_repairs[1].severity, "warning");
}

#[test]
fn a_technician_is_busy_from_two_jobs_and_overloaded_at_five() {
    let employees = [employee("t1", "technician"), employee("s1", "sales")];
    let mut repairs: Vec<Repair> = (0..5)
        .map(|i| repair(&format!("j{i}"), "in_repair", Some("t1"), NOW, 100))
        .collect();
    let busy = compute(&input(&repairs[..2], &employees));
    assert_eq!(busy.technicians.len(), 1);
    assert_eq!(busy.technicians[0].status, "busy");
    assert_eq!(busy.technicians[0].capacity_percent, 40);
    repairs.push(repair("done", "ready", Some("t1"), NOW, 100));
    let full = compute(&input(&repairs, &employees));
    assert_eq!(full.technicians[0].status, "overloaded");
    assert_eq!(full.technicians[0].capacity_percent, 100);
    assert_eq!(full.technicians[0].completed_today_count, 1);
}

#[test]
fn pending_stock_quotations_are_tracked_and_flagged_ready_when_replenished() {
    let quote: Quotation = from(json!({
        "id": "q1",
        "key": "quo_1",
        "quotationNumber": "QUO-0001",
        "customerName": "Kamal",
        "items": [{
            "productKey": "prod_1",
            "name": "Screen Protector",
            "quantity": 3,
            "unitPriceCents": 1500,
            "discountCents": 0,
            "totalCents": 4500
        }],
        "subtotalCents": 4500,
        "discountType": "fixed",
        "discountValue": 0.0,
        "discountCents": 0,
        "totalCents": 4500,
        "validUntil": "2026-11-26",
        "status": "active",
        "isExpired": false,
        "hasOutOfStockItems": true,
        "createdAt": NOW,
        "updatedAt": NOW
    }));

    let empty_stock_product: Product = from(json!({
        "id": "p1", "key": "prod_1", "sku": "SP-01", "name": "Screen Protector",
        "categoryKey": "cat_1", "category": "Accessories",
        "subcategoryKey": "sub_1", "subcategory": "Protectors",
        "stockQuantity": 0, "minStockThreshold": 5,
        "priceTiers": [], "isSerialized": false,
        "createdAt": NOW, "updatedAt": NOW, "version": 1
    }));

    let mut inp = input(&[], &[]);
    inp.active_quotations = std::slice::from_ref(&quote);
    inp.products = std::slice::from_ref(&empty_stock_product);

    let pulse_waiting = compute(&inp);
    assert_eq!(pulse_waiting.pending_stock_quotations.len(), 1);
    assert!(!pulse_waiting.pending_stock_quotations[0].is_ready_to_fill);
    assert!(!pulse_waiting.pending_stock_quotations[0].items[0].has_sufficient_stock);

    // When stock is replenished to 5:
    let replenished_product: Product = from(json!({
        "id": "p1", "key": "prod_1", "sku": "SP-01", "name": "Screen Protector",
        "categoryKey": "cat_1", "category": "Accessories",
        "subcategoryKey": "sub_1", "subcategory": "Protectors",
        "stockQuantity": 5, "minStockThreshold": 5,
        "priceTiers": [], "isSerialized": false,
        "createdAt": NOW, "updatedAt": NOW, "version": 1
    }));
    inp.products = std::slice::from_ref(&replenished_product);

    let pulse_ready = compute(&inp);
    assert_eq!(pulse_ready.pending_stock_quotations.len(), 1);
    assert!(pulse_ready.pending_stock_quotations[0].is_ready_to_fill);
    assert!(pulse_ready.pending_stock_quotations[0].items[0].has_sufficient_stock);
}
