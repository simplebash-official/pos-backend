use super::op::Op;
use super::rng::Rng;

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
