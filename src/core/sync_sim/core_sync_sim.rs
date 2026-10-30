use super::sim_replica::SimReplica;
use crate::core::sync_merge::RowMeta;
use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::Value;

/// Result of applying one change to a replica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyResult {
    Applied,
    Duplicate,
    KeepLocal,
    Rejected(&'static str),
}

pub(super) fn get_i64(p: &Value, key: &str) -> i64 {
    p.get(key).and_then(Value::as_i64).unwrap_or(0)
}

pub(super) fn get_str(p: &Value, key: &str) -> String {
    p.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .expect("valid millis")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// One step of a scenario. `sel` fields pick among the rows visible to the
/// acting device at run time.
#[derive(Debug, Clone)]
pub enum Op {
    NewProduct {
        dev: usize,
        price: i64,
    },
    NewCustomer {
        dev: usize,
    },
    PriceEdit {
        dev: usize,
        sel: u64,
        price: i64,
    },
    DeleteProduct {
        dev: usize,
        sel: u64,
    },
    StockAdjust {
        dev: usize,
        sel: u64,
        delta: i64,
    },
    Sale {
        dev: usize,
        sel_product: u64,
        sel_customer: u64,
        qty: i64,
        credit: bool,
        deposit_percent: i64,
    },
    Payment {
        dev: usize,
        sel_invoice: u64,
        amount: i64,
    },
    Return {
        dev: usize,
        sel_invoice: u64,
        qty: i64,
    },
    VoidInvoice {
        dev: usize,
        sel_invoice: u64,
    },
    NoteInvoice {
        dev: usize,
        sel_invoice: u64,
        note: u64,
    },
    Offline {
        dev: usize,
    },
    Online {
        dev: usize,
    },
    Skew {
        dev: usize,
        ms: i64,
        known: bool,
    },
    Sync {
        dev: usize,
    },
}

/// SplitMix64: tiny, seedable, no dependencies.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (0 when `n == 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

/// Seeded scenario generator.
#[derive(Debug)]
pub struct Scenario;

impl Scenario {
    pub fn generate(seed: u64, devices: usize, ops: usize) -> Vec<Op> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::with_capacity(ops + devices * 2);
        // Every device starts with a product and a customer so later ops have targets.
        for dev in 0..devices {
            out.push(Op::NewProduct {
                dev,
                price: 100 + rng.below(900) as i64,
            });
            out.push(Op::NewCustomer { dev });
        }
        for _ in 0..ops {
            let dev = rng.below(devices as u64) as usize;
            let sel = rng.next_u64();
            let op = match rng.below(100) {
                0..=7 => Op::NewProduct {
                    dev,
                    price: 50 + rng.below(950) as i64,
                },
                8..=11 => Op::NewCustomer { dev },
                12..=21 => Op::PriceEdit {
                    dev,
                    sel,
                    price: 50 + rng.below(950) as i64,
                },
                22..=24 => Op::DeleteProduct { dev, sel },
                25..=32 => Op::StockAdjust {
                    dev,
                    sel,
                    delta: rng.below(41) as i64 - 10,
                },
                33..=52 => Op::Sale {
                    dev,
                    sel_product: sel,
                    sel_customer: rng.next_u64(),
                    qty: 1 + rng.below(4) as i64,
                    credit: rng.chance(35),
                    deposit_percent: rng.below(101) as i64,
                },
                53..=59 => Op::Payment {
                    dev,
                    sel_invoice: sel,
                    amount: 10 + rng.below(400) as i64,
                },
                60..=65 => Op::Return {
                    dev,
                    sel_invoice: sel,
                    qty: 1 + rng.below(2) as i64,
                },
                66..=68 => Op::VoidInvoice {
                    dev,
                    sel_invoice: sel,
                },
                69..=72 => Op::NoteInvoice {
                    dev,
                    sel_invoice: sel,
                    note: rng.below(1000),
                },
                73..=79 => Op::Offline { dev },
                80..=86 => Op::Online { dev },
                87..=89 => Op::Skew {
                    dev,
                    ms: rng.below(180_001) as i64 - 90_000,
                    known: rng.chance(50),
                },
                _ => Op::Sync { dev },
            };
            out.push(op);
        }
        out
    }
}

/// One simulated device plus its link state to the hub.
#[derive(Debug, Clone)]
pub struct SimDevice {
    pub replica: SimReplica,
    pub online: bool,
    /// How much of the hub log this device has consumed.
    pub cursor: usize,
    pub(super) counter: u64,
}

/// One stored row: merge metadata plus the DTO payload.
#[derive(Debug, Clone)]
pub struct SimRow {
    pub meta: RowMeta,
    pub payload: Value,
}
