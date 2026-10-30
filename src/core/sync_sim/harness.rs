use super::apply_result::ApplyResult;
use super::get_i64::get_i64;
use super::get_str::get_str;
use super::iso::iso;
use super::op::Op;
use super::rng::Rng;
use super::sim_device::SimDevice;
use super::sim_replica::SimReplica;

use crate::domain::sync_v2::{ChangeOp, ChangeRecord};
use crate::modules::sync::resources::spec;

use serde_json::{Value, json};

/// Hub-and-spoke run: devices push their outbox to the hub (the cloud
/// replica), the hub logs what it applied, devices pull others' entries.
#[derive(Debug)]
pub struct Harness {
    pub hub: SimReplica,
    pub devices: Vec<SimDevice>,
    /// Applied changes in hub order, with the pushing device.
    pub log: Vec<(String, ChangeRecord)>,
    pub now_ms: i64,
    pub(super) rng: Rng,
}

impl Harness {
    pub fn new(devices: usize, seed: u64) -> Self {
        Harness {
            hub: SimReplica::new("cloud"),
            devices: (0..devices)
                .map(|i| SimDevice {
                    replica: SimReplica::new(&format!("dev_{i}")),
                    online: true,
                    cursor: 0,
                    counter: 0,
                })
                .collect(),
            log: Vec::new(),
            now_ms: 1_800_000_000_000,
            rng: Rng::new(seed ^ 0xA5A5_A5A5),
        }
    }

    pub(super) fn fresh_key(&mut self, dev: usize, prefix: &str) -> String {
        let d = &mut self.devices[dev];
        d.counter += 1;
        format!("{prefix}_{dev}_{}", d.counter)
    }

    /// Runs one op on its device.
    pub fn step(&mut self, op: &Op) {
        self.now_ms += 1 + self.rng.below(400) as i64;
        let now = self.now_ms;
        match *op {
            Op::NewProduct { dev, price } => {
                let key = self.fresh_key(dev, "prod");
                let payload = json!({
                    "id": key, "name": format!("Product {key}"), "priceCents": price,
                    "stockQuantity": 0
                });
                self.devices[dev].replica.local_write(
                    "products",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::NewCustomer { dev } => {
                let key = self.fresh_key(dev, "cus");
                let payload = json!({
                    "id": key, "name": format!("Customer {key}"),
                    "totalPurchasesCents": 0, "outstandingBalanceCents": 0
                });
                self.devices[dev].replica.local_write(
                    "customers",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::PriceEdit { dev, sel, price } => {
                let products = self.devices[dev].replica.visible("products");
                if products.is_empty() {
                    return;
                }
                let (key, mut payload) = products[(sel % products.len() as u64) as usize].clone();
                payload["priceCents"] = json!(price);
                self.devices[dev].replica.local_write(
                    "products",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::DeleteProduct { dev, sel } => {
                let products = self.devices[dev].replica.visible("products");
                if products.len() < 2 {
                    return;
                }
                let (key, _) = products[(sel % products.len() as u64) as usize].clone();
                self.devices[dev].replica.local_write(
                    "products",
                    &key,
                    ChangeOp::Delete,
                    None,
                    now,
                );
            }
            Op::StockAdjust { dev, sel, delta } => {
                let products = self.devices[dev].replica.visible("products");
                if products.is_empty() || delta == 0 {
                    return;
                }
                let (pkey, _) = products[(sel % products.len() as u64) as usize].clone();
                let key = self.fresh_key(dev, "sm");
                let payload = json!({ "id": key, "productId": pkey, "quantityDelta": delta });
                self.devices[dev].replica.local_write(
                    "stockMovements",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Sale {
                dev,
                sel_product,
                sel_customer,
                qty,
                credit,
                deposit_percent,
            } => {
                let products = self.devices[dev].replica.visible("products");
                let customers = self.devices[dev].replica.visible("customers");
                if products.is_empty() || customers.is_empty() {
                    return;
                }
                let (pkey, product) =
                    products[(sel_product % products.len() as u64) as usize].clone();
                let (ckey, _) = customers[(sel_customer % customers.len() as u64) as usize].clone();
                let unit = get_i64(&product, "priceCents");
                let total = unit * qty;
                let deposit = if credit {
                    total * deposit_percent / 100
                } else {
                    total
                };
                let status = if deposit >= total {
                    "paid"
                } else if deposit > 0 {
                    "partially_paid"
                } else {
                    "pending"
                };
                let ikey = self.fresh_key(dev, "inv");
                let invoice = json!({
                    "id": ikey, "customerKey": ckey, "totalCents": total, "isCredit": credit,
                    "status": status, "notes": null,
                    "items": [ { "productKey": pkey, "unitPriceCents": unit, "quantity": qty, "returnedQuantity": 0 } ],
                    "refundedCents": 0, "creditNoteCount": 0
                });
                let r = &mut self.devices[dev].replica;
                r.local_write("invoices", &ikey, ChangeOp::Upsert, Some(invoice), now);
                if deposit > 0 {
                    let pay = self.fresh_key(dev, "pay");
                    let payload = json!({ "id": pay, "invoiceKey": ikey, "amountCents": deposit });
                    self.devices[dev].replica.local_write(
                        "payments",
                        &pay,
                        ChangeOp::Upsert,
                        Some(payload),
                        now,
                    );
                }
                let sm = self.fresh_key(dev, "sm");
                let payload = json!({ "id": sm, "productId": pkey, "quantityDelta": -qty });
                self.devices[dev].replica.local_write(
                    "stockMovements",
                    &sm,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Payment {
                dev,
                sel_invoice,
                amount,
            } => {
                let invoices: Vec<_> = self.devices[dev]
                    .replica
                    .visible("invoices")
                    .into_iter()
                    .filter(|(_, p)| {
                        p.get("isCredit").and_then(Value::as_bool).unwrap_or(false)
                            && matches!(get_str(p, "status").as_str(), "pending" | "partially_paid")
                    })
                    .collect();
                if invoices.is_empty() {
                    return;
                }
                let (ikey, _) = invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                let pay = self.fresh_key(dev, "pay");
                let payload = json!({ "id": pay, "invoiceKey": ikey, "amountCents": amount });
                self.devices[dev].replica.local_write(
                    "payments",
                    &pay,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Return {
                dev,
                sel_invoice,
                qty,
            } => {
                let invoices: Vec<_> = self.devices[dev]
                    .replica
                    .visible("invoices")
                    .into_iter()
                    .filter(|(_, p)| get_str(p, "status") != "voided")
                    .collect();
                if invoices.is_empty() {
                    return;
                }
                let (ikey, invoice) =
                    invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                let item = invoice["items"][0].clone();
                let unit = get_i64(&item, "unitPriceCents");
                let pkey = get_str(&item, "productKey");
                let cn = self.fresh_key(dev, "cn");
                let payload = json!({
                    "id": cn, "invoiceKey": ikey, "customerKey": get_str(&invoice, "customerKey"),
                    "refundCashCents": unit * qty, "balanceReductionCents": 0,
                    "status": "resolved",
                    "returnedItems": [ { "productKey": pkey, "quantity": qty } ]
                });
                self.devices[dev].replica.local_write(
                    "creditNotes",
                    &cn,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
                let sm = self.fresh_key(dev, "sm");
                let payload = json!({ "id": sm, "productId": pkey, "quantityDelta": qty });
                self.devices[dev].replica.local_write(
                    "stockMovements",
                    &sm,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::VoidInvoice { dev, sel_invoice } => {
                let invoices: Vec<_> = self.devices[dev]
                    .replica
                    .visible("invoices")
                    .into_iter()
                    .filter(|(_, p)| get_str(p, "status") != "voided")
                    .collect();
                if invoices.is_empty() {
                    return;
                }
                let (ikey, mut payload) =
                    invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                payload["status"] = json!("voided");
                payload["voidedAt"] = json!(iso(now));
                payload["voidedBy"] = json!(format!("user_{dev}"));
                payload["voidedReason"] = json!("sim");
                self.devices[dev].replica.local_write(
                    "invoices",
                    &ikey,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::NoteInvoice {
                dev,
                sel_invoice,
                note,
            } => {
                let invoices = self.devices[dev].replica.visible("invoices");
                if invoices.is_empty() {
                    return;
                }
                let (ikey, mut payload) =
                    invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                payload["notes"] = json!(format!("note {note}"));
                self.devices[dev].replica.local_write(
                    "invoices",
                    &ikey,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Offline { dev } => self.devices[dev].online = false,
            Op::Online { dev } => self.devices[dev].online = true,
            Op::Skew { dev, ms, known } => {
                self.devices[dev].replica.clock_skew_ms = ms;
                self.devices[dev].replica.skew_known = known;
            }
            Op::Sync { dev } => self.sync(dev),
        }
    }

    /// Push the outbox to the hub, then pull what other devices sent. No-op offline.
    pub fn sync(&mut self, dev: usize) {
        if !self.devices[dev].online {
            return;
        }
        let device_id = self.devices[dev].replica.device_id.clone();
        let pending = std::mem::take(&mut self.devices[dev].replica.pending);
        if !pending.is_empty() {
            let mut order: Vec<usize> = (0..pending.len()).collect();
            order.sort_by_key(|&i| {
                (
                    spec(&pending[i].resource)
                        .map(|s| s.order)
                        .unwrap_or(u8::MAX),
                    pending[i].key.clone(),
                )
            });
            for i in order {
                let rec = &pending[i];
                if self.hub.apply(rec, Some(self.now_ms)) == ApplyResult::Applied {
                    self.log.push((device_id.clone(), rec.clone()));
                }
            }
            self.hub.derive();
        }
        let cursor = self.devices[dev].cursor;
        let incoming: Vec<ChangeRecord> = self.log[cursor..]
            .iter()
            .filter(|(origin, _)| *origin != device_id)
            .map(|(_, rec)| rec.clone())
            .collect();
        self.devices[dev].cursor = self.log.len();
        if !incoming.is_empty() {
            self.devices[dev].replica.apply_batch(&incoming, None);
        }
    }

    /// Brings every device online and syncs until nothing moves.
    pub fn settle(&mut self) {
        for d in &mut self.devices {
            d.online = true;
        }
        for _ in 0..20 {
            for dev in 0..self.devices.len() {
                self.sync(dev);
            }
            let quiet = self
                .devices
                .iter()
                .all(|d| d.replica.pending.is_empty() && d.cursor == self.log.len());
            if quiet {
                return;
            }
        }
        panic!("sync did not settle");
    }

    pub fn run(&mut self, ops: &[Op]) {
        for op in ops {
            self.step(op);
        }
        self.settle();
    }
}
