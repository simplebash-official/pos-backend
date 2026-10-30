use super::core_sync_sim::ApplyResult;
use super::core_sync_sim::Op;
use super::core_sync_sim::Rng;
use super::core_sync_sim::Scenario;
use super::core_sync_sim::get_i64;
use super::core_sync_sim::get_str;
use super::harness::Harness;
use super::sim_replica::SimReplica;

use crate::domain::sync_v2::{ChangeOp, ChangeRecord, ConflictKind};

use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Fails with just the rows that differ (both sides), not the whole state.
fn assert_same(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>, label: &str) {
    let mut diffs = Vec::new();
    for key in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
        if left.get(key) != right.get(key) {
            diffs.push(format!(
                "{key}\n   left : {:?}\n   right: {:?}",
                left.get(key),
                right.get(key)
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "{label}: {} rows differ:\n{}",
        diffs.len(),
        diffs.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
    );
}

fn naive_stock(replica: &SimReplica, product_key: &str) -> i64 {
    replica
        .visible("stockMovements")
        .iter()
        .filter(|(_, p)| get_str(p, "productId") == product_key)
        .map(|(_, p)| get_i64(p, "quantityDelta"))
        .sum()
}

fn assert_invariants(replica: &SimReplica, label: &str) {
    for (key, p) in replica.visible("products") {
        assert_eq!(
            get_i64(&p, "stockQuantity"),
            naive_stock(replica, &key),
            "{label}: stock of {key}"
        );
    }
    let invoices = replica.visible("invoices");
    let payments = replica.visible("payments");
    let credit_notes = replica.visible("creditNotes");
    for (key, c) in replica.visible("customers") {
        let mut purchases = 0;
        let mut outstanding = 0;
        for (ikey, inv) in &invoices {
            if get_str(inv, "customerKey") != key || get_str(inv, "status") == "voided" {
                continue;
            }
            let total = get_i64(inv, "totalCents");
            purchases += total;
            if inv["isCredit"].as_bool().unwrap_or(false) {
                let paid: i64 = payments
                    .iter()
                    .filter(|(_, p)| get_str(p, "invoiceKey") == *ikey)
                    .map(|(_, p)| get_i64(p, "amountCents"))
                    .sum();
                outstanding += total - paid;
            }
        }
        for (_, cn) in &credit_notes {
            if get_str(cn, "customerKey") == key && get_str(cn, "status") != "voided" {
                purchases -= get_i64(cn, "refundCashCents");
            }
        }
        assert_eq!(
            get_i64(&c, "totalPurchasesCents"),
            purchases,
            "{label}: purchases of {key}"
        );
        assert_eq!(
            get_i64(&c, "outstandingBalanceCents"),
            outstanding,
            "{label}: balance of {key}"
        );
    }
    for (ikey, inv) in &invoices {
        let (mut sum, mut count) = (0, 0);
        for (_, cn) in &credit_notes {
            if get_str(cn, "invoiceKey") == *ikey && get_str(cn, "status") != "voided" {
                sum += get_i64(cn, "refundCashCents") + get_i64(cn, "balanceReductionCents");
                count += 1;
            }
        }
        assert_eq!(
            get_i64(inv, "refundedCents"),
            sum,
            "{label}: refunded of {ikey}"
        );
        assert_eq!(
            get_i64(inv, "creditNoteCount"),
            count,
            "{label}: count of {ikey}"
        );
    }
}

#[test]
fn rng_is_deterministic_and_shuffle_is_a_permutation() {
    let mut a = Rng::new(7);
    let mut b = Rng::new(7);
    assert_eq!(a.next_u64(), b.next_u64());
    let mut v: Vec<u32> = (0..50).collect();
    Rng::new(1).shuffle(&mut v);
    let mut sorted = v.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..50).collect::<Vec<_>>());
    assert_ne!(v, sorted);
}

#[test]
fn scenario_generation_is_deterministic() {
    let a = format!("{:?}", Scenario::generate(3, 3, 80));
    let b = format!("{:?}", Scenario::generate(3, 3, 80));
    assert_eq!(a, b);
    assert_ne!(a, format!("{:?}", Scenario::generate(4, 3, 80)));
}

#[test]
fn a_sale_then_a_return_derive_stock_and_balances() {
    let mut h = Harness::new(1, 1);
    h.run(&[
        Op::NewProduct { dev: 0, price: 200 },
        Op::NewCustomer { dev: 0 },
        Op::StockAdjust {
            dev: 0,
            sel: 0,
            delta: 10,
        },
        Op::Sale {
            dev: 0,
            sel_product: 0,
            sel_customer: 0,
            qty: 3,
            credit: true,
            deposit_percent: 50,
        },
        Op::Return {
            dev: 0,
            sel_invoice: 0,
            qty: 1,
        },
    ]);
    let r = &h.devices[0].replica;
    assert_invariants(r, "single device");
    let product = &r.visible("products")[0].1;
    assert_eq!(get_i64(product, "stockQuantity"), 10 - 3 + 1);
    let customer = &r.visible("customers")[0].1;
    // sale 600 total, deposit 300, return refunds 200
    assert_eq!(get_i64(customer, "totalPurchasesCents"), 600 - 200);
    assert_eq!(get_i64(customer, "outstandingBalanceCents"), 300);
    assert_eq!(h.hub.canonical(), r.canonical());
}

#[test]
fn concurrent_price_edits_converge_to_the_later_write() {
    let mut h = Harness::new(2, 2);
    h.step(&Op::NewProduct { dev: 0, price: 100 });
    h.settle();
    h.devices[0].online = false;
    h.devices[1].online = false;
    h.step(&Op::PriceEdit {
        dev: 0,
        sel: 0,
        price: 111,
    });
    h.step(&Op::PriceEdit {
        dev: 1,
        sel: 0,
        price: 222,
    });
    h.settle();
    let a = h.devices[0].replica.canonical();
    assert_eq!(a, h.devices[1].replica.canonical());
    assert_eq!(a, h.hub.canonical());
    let price = get_i64(&h.hub.visible("products")[0].1, "priceCents");
    assert_eq!(price, 222, "device 1 edited last");
    assert!(
        h.hub
            .conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::LwwLoser)
            || h.devices.iter().any(|d| d
                .replica
                .conflicts
                .iter()
                .any(|c| c.kind == ConflictKind::LwwLoser))
    );
}

#[test]
fn two_offline_sales_of_the_last_unit_converge_and_flag_negative_stock() {
    let mut h = Harness::new(2, 3);
    h.step(&Op::NewProduct { dev: 0, price: 100 });
    h.step(&Op::NewCustomer { dev: 0 });
    h.step(&Op::StockAdjust {
        dev: 0,
        sel: 0,
        delta: 1,
    });
    h.settle();
    h.devices[0].online = false;
    h.devices[1].online = false;
    for dev in 0..2 {
        h.step(&Op::Sale {
            dev,
            sel_product: 0,
            sel_customer: 0,
            qty: 1,
            credit: false,
            deposit_percent: 100,
        });
    }
    h.settle();
    for d in &h.devices {
        assert_invariants(&d.replica, &d.replica.device_id);
        assert_eq!(
            get_i64(&d.replica.visible("products")[0].1, "stockQuantity"),
            -1
        );
    }
    assert_eq!(
        h.devices[0].replica.canonical(),
        h.devices[1].replica.canonical()
    );
    assert!(h.devices.iter().any(|d| {
        d.replica
            .conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::NegativeStock)
    }));
}

#[test]
fn two_devices_returning_the_same_invoice_flag_an_over_refund_and_converge() {
    let mut h = Harness::new(2, 4);
    h.step(&Op::NewProduct { dev: 0, price: 100 });
    h.step(&Op::NewCustomer { dev: 0 });
    h.step(&Op::StockAdjust {
        dev: 0,
        sel: 0,
        delta: 5,
    });
    h.step(&Op::Sale {
        dev: 0,
        sel_product: 0,
        sel_customer: 0,
        qty: 1,
        credit: false,
        deposit_percent: 100,
    });
    h.settle();
    h.devices[0].online = false;
    h.devices[1].online = false;
    for dev in 0..2 {
        h.step(&Op::Return {
            dev,
            sel_invoice: 0,
            qty: 1,
        });
    }
    h.settle();
    assert_eq!(
        h.devices[0].replica.canonical(),
        h.devices[1].replica.canonical()
    );
    let inv = &h.devices[0].replica.visible("invoices")[0].1;
    assert_eq!(get_i64(inv, "refundedCents"), 200);
    assert_eq!(get_i64(inv, "creditNoteCount"), 2);
    assert!(h.devices.iter().any(|d| {
        d.replica
            .conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::OverRefund)
    }));
}

#[test]
fn concurrent_void_and_note_keep_the_void_and_the_latest_note() {
    let mut h = Harness::new(2, 5);
    h.step(&Op::NewProduct { dev: 0, price: 100 });
    h.step(&Op::NewCustomer { dev: 0 });
    h.step(&Op::StockAdjust {
        dev: 0,
        sel: 0,
        delta: 5,
    });
    h.step(&Op::Sale {
        dev: 0,
        sel_product: 0,
        sel_customer: 0,
        qty: 1,
        credit: false,
        deposit_percent: 100,
    });
    h.settle();
    h.devices[0].online = false;
    h.devices[1].online = false;
    h.step(&Op::VoidInvoice {
        dev: 0,
        sel_invoice: 0,
    });
    h.step(&Op::NoteInvoice {
        dev: 1,
        sel_invoice: 0,
        note: 42,
    });
    h.settle();
    let inv = h.hub.visible("invoices")[0].1.clone();
    assert_eq!(get_str(&inv, "status"), "voided");
    assert_eq!(get_str(&inv, "notes"), "note 42");
    assert_eq!(
        h.devices[0].replica.canonical(),
        h.devices[1].replica.canonical()
    );
    assert_eq!(h.devices[0].replica.canonical(), h.hub.canonical());
}

#[test]
fn deleted_product_stays_deleted_on_every_replica() {
    let mut h = Harness::new(2, 6);
    h.run(&[
        Op::NewProduct { dev: 0, price: 100 },
        Op::NewProduct { dev: 1, price: 100 },
    ]);
    h.step(&Op::DeleteProduct { dev: 0, sel: 0 });
    h.settle();
    for d in &h.devices {
        assert_eq!(d.replica.visible("products").len(), 1);
    }
    assert_eq!(
        h.devices[0].replica.canonical(),
        h.devices[1].replica.canonical()
    );
}

#[test]
fn scenarios_converge_across_many_seeds_and_keep_ledger_invariants() {
    for seed in 0..40u64 {
        let devices = 2 + (seed % 3) as usize;
        let ops = Scenario::generate(seed, devices, 250);
        let mut h = Harness::new(devices, seed);
        h.run(&ops);
        let hub = h.hub.canonical();
        assert_invariants(&h.hub, &format!("seed {seed} hub"));
        for d in &h.devices {
            let label = format!("seed {seed} {}", d.replica.device_id);
            assert_same(
                &d.replica.canonical(),
                &hub,
                &format!("{label} diverged from the cloud"),
            );
            assert_invariants(&d.replica, &label);
        }
    }
}

#[test]
fn any_delivery_order_of_the_applied_changes_reaches_the_same_state() {
    for seed in 0..25u64 {
        let devices = 2 + (seed % 3) as usize;
        let mut h = Harness::new(devices, seed);
        h.run(&Scenario::generate(seed, devices, 200));
        let expected = h.hub.canonical();
        let mut records: Vec<ChangeRecord> = h.log.iter().map(|(_, r)| r.clone()).collect();
        let mut rng = Rng::new(seed + 1000);
        for round in 0..6 {
            rng.shuffle(&mut records);
            let mut replica = SimReplica::new("verifier");
            for rec in &records {
                replica.apply(rec, None);
            }
            replica.derive();
            assert_same(
                &replica.canonical(),
                &expected,
                &format!("seed {seed} round {round}"),
            );
            assert_invariants(&replica, &format!("seed {seed} round {round}"));
        }
    }
}

#[test]
fn applying_the_same_changes_twice_changes_nothing() {
    for seed in 0..15u64 {
        let mut h = Harness::new(3, seed);
        h.run(&Scenario::generate(seed, 3, 200));
        let records: Vec<ChangeRecord> = h.log.iter().map(|(_, r)| r.clone()).collect();
        let mut replica = SimReplica::new("verifier");
        replica.apply_batch(&records, None);
        let once = replica.canonical();
        let second = replica.apply_batch(&records, None);
        assert_eq!(replica.canonical(), once, "seed {seed}");
        assert!(
            second
                .iter()
                .all(|r| matches!(r, ApplyResult::Duplicate | ApplyResult::KeepLocal)),
            "seed {seed}: a repeat was applied again"
        );
    }
}

#[test]
fn far_future_timestamps_are_clamped_by_the_receiver_only() {
    let mut hub = SimReplica::new("cloud");
    let now = 1_800_000_000_000;
    let rec = ChangeRecord {
        resource: "products".into(),
        key: "p1".into(),
        op: ChangeOp::Upsert,
        version: 1,
        updated_at: Utc.timestamp_millis_opt(now + 3_600_000).single().unwrap(),
        device_id: "dev_x".into(),
        payload: Some(json!({ "key": "p1", "id": "p1", "priceCents": 5 })),
    };
    assert_eq!(hub.apply(&rec, Some(now)), ApplyResult::Applied);
    let row = &hub.rows[&("products".to_string(), "p1".to_string())];
    assert_eq!(row.meta.updated_at_ms, now);
    let mut device = SimReplica::new("dev_y");
    device.apply(&rec, None);
    let row = &device.rows[&("products".to_string(), "p1".to_string())];
    assert_eq!(row.meta.updated_at_ms, now + 3_600_000);
}
