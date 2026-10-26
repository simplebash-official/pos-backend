// Counting a supplier payment against the bills (receipts) it pays: what is owed together, how a
// payment spreads over the open bills oldest first, and what is wrong with a split the person made
// by hand. Pure; the handler reads the supplier's open bills. The save checks the same rules again
// (`purchases::service::settlement::prepare_settlements`).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::core::error::{AppError, AppResult};

const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_SETTLEMENTS: usize = 200;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementPart {
    pub receipt_key: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementRequest {
    pub supplier_key: String,
    /// What is being paid; 0 until typed.
    pub payment_cents: i64,
    /// How the person split the payment over bills.
    pub settlements: Vec<CalcSettlementPart>,
}

/// An open bill, oldest first.
pub struct OpenBill {
    pub key: String,
    pub due_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SettlementBlockerCode {
    /// The bill is not owed any more (or is not this supplier's).
    BillNotOwed,
    /// An amount of zero or less against a bill.
    AmountMissing,
    /// More than the bill still owes.
    OverBill,
    /// The same bill listed twice.
    BillRepeated,
    /// More put against bills than is being paid.
    OverPayment,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettlementBlocker {
    pub code: SettlementBlockerCode,
    /// The bill, for the bill blockers.
    pub receipt_key: Option<String>,
    /// What the bill still owes, for `over_bill`.
    pub due_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementReply {
    /// What every open bill still owes together.
    pub owed_cents: i64,
    /// What the person's split puts against bills.
    pub allocated_cents: i64,
    /// The payment spread over the open bills, oldest first, each taking at most what it owes. What
    /// is left after every bill is paid stays unallocated (it still lowers what the supplier is owed).
    pub oldest_first: Vec<CalcSettlementPartReply>,
    /// Every open bill paid in full: what "Pay everything owed" puts against bills.
    pub pay_everything: Vec<CalcSettlementPartReply>,
    /// What ticking each open bill would put against it now: what is left of the payment, or the
    /// whole bill when nothing is left, never more than the bill owes.
    pub if_ticked: Vec<CalcSettlementPartReply>,
    /// What is wrong with the person's split, in the order to say it; empty means it is fine.
    pub blockers: Vec<SettlementBlocker>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementPartReply {
    pub receipt_key: String,
    pub amount_cents: i64,
}

pub fn compute(request: &CalcSettlementRequest, open: &[OpenBill]) -> AppResult<CalcSettlementReply> {
    if !(0..=MAX_CENTS).contains(&request.payment_cents) {
        return Err(AppError::validation("The payment is out of range"));
    }
    if request.settlements.len() > MAX_SETTLEMENTS {
        return Err(AppError::validation("Too many bills"));
    }
    if request
        .settlements
        .iter()
        .any(|part| part.amount_cents > MAX_CENTS)
    {
        return Err(AppError::validation("An amount is out of range"));
    }

    let owed_cents = open.iter().map(|bill| bill.due_cents).sum();
    let mut left = request.payment_cents;
    let mut oldest_first = Vec::new();
    for bill in open {
        let take = left.min(bill.due_cents);
        if take > 0 {
            oldest_first.push(CalcSettlementPartReply {
                receipt_key: bill.key.clone(),
                amount_cents: take,
            });
            left -= take;
        }
    }
    let pay_everything = open
        .iter()
        .filter(|bill| bill.due_cents > 0)
        .map(|bill| CalcSettlementPartReply {
            receipt_key: bill.key.clone(),
            amount_cents: bill.due_cents,
        })
        .collect();

    let mut blockers = Vec::new();
    let mut seen = HashSet::new();
    let mut allocated_cents: i64 = 0;
    for part in &request.settlements {
        let blocker = |code, due_cents| SettlementBlocker {
            code,
            receipt_key: Some(part.receipt_key.clone()),
            due_cents,
        };
        if !seen.insert(part.receipt_key.as_str()) {
            blockers.push(blocker(SettlementBlockerCode::BillRepeated, None));
            continue;
        }
        let Some(bill) = open.iter().find(|bill| bill.key == part.receipt_key) else {
            blockers.push(blocker(SettlementBlockerCode::BillNotOwed, None));
            continue;
        };
        if part.amount_cents <= 0 {
            blockers.push(blocker(SettlementBlockerCode::AmountMissing, None));
        } else if part.amount_cents > bill.due_cents {
            blockers.push(blocker(
                SettlementBlockerCode::OverBill,
                Some(bill.due_cents),
            ));
        }
        allocated_cents += part.amount_cents.max(0);
    }
    if allocated_cents > request.payment_cents {
        blockers.push(SettlementBlocker {
            code: SettlementBlockerCode::OverPayment,
            receipt_key: None,
            due_cents: None,
        });
    }

    let room = (request.payment_cents - allocated_cents).max(0);
    let if_ticked = open
        .iter()
        .filter(|bill| bill.due_cents > 0)
        .map(|bill| CalcSettlementPartReply {
            receipt_key: bill.key.clone(),
            amount_cents: if room > 0 { room.min(bill.due_cents) } else { bill.due_cents },
        })
        .collect();

    Ok(CalcSettlementReply {
        owed_cents,
        allocated_cents,
        oldest_first,
        pay_everything,
        if_ticked,
        blockers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bills() -> Vec<OpenBill> {
        vec![
            OpenBill { key: "prc_old".into(), due_cents: 30_000 },
            OpenBill { key: "prc_mid".into(), due_cents: 50_000 },
            OpenBill { key: "prc_new".into(), due_cents: 20_000 },
        ]
    }

    fn request(payment: i64, parts: &[(&str, i64)]) -> CalcSettlementRequest {
        CalcSettlementRequest {
            supplier_key: "sup_1".into(),
            payment_cents: payment,
            settlements: parts
                .iter()
                .map(|(key, cents)| CalcSettlementPart {
                    receipt_key: (*key).into(),
                    amount_cents: *cents,
                })
                .collect(),
        }
    }

    fn spread(parts: &[CalcSettlementPartReply]) -> Vec<(&str, i64)> {
        parts.iter().map(|p| (p.receipt_key.as_str(), p.amount_cents)).collect()
    }

    #[test]
    fn ticking_a_bill_takes_what_is_left_of_the_payment_or_the_whole_bill() {
        let reply = compute(&request(40_000, &[("prc_old", 30_000)]), &bills()).unwrap();
        assert_eq!(spread(&reply.if_ticked), [("prc_old", 10_000), ("prc_mid", 10_000), ("prc_new", 10_000)]);
        let none_left = compute(&request(30_000, &[("prc_old", 30_000)]), &bills()).unwrap();
        assert_eq!(spread(&none_left.if_ticked)[1], ("prc_mid", 50_000));
    }

    #[test]
    fn a_payment_pays_the_oldest_bills_first() {
        let reply = compute(&request(60_000, &[]), &bills()).unwrap();
        assert_eq!(reply.owed_cents, 100_000);
        assert_eq!(spread(&reply.oldest_first), vec![("prc_old", 30_000), ("prc_mid", 30_000)]);
        let more = compute(&request(150_000, &[]), &bills()).unwrap();
        // Everything is paid; the rest stays unallocated.
        assert_eq!(spread(&more.oldest_first).len(), 3);
        assert_eq!(more.oldest_first.iter().map(|p| p.amount_cents).sum::<i64>(), 100_000);
        assert_eq!(spread(&more.pay_everything), vec![("prc_old", 30_000), ("prc_mid", 50_000), ("prc_new", 20_000)]);
    }

    #[test]
    fn nothing_is_spread_without_a_payment() {
        assert!(compute(&request(0, &[]), &bills()).unwrap().oldest_first.is_empty());
    }

    #[test]
    fn a_split_is_checked_bill_by_bill() {
        let reply = compute(
            &request(
                40_000,
                &[("prc_old", 30_000), ("prc_gone", 1_000), ("prc_mid", 0), ("prc_new", 25_000), ("prc_old", 5)],
            ),
            &bills(),
        )
        .unwrap();
        let codes: Vec<_> = reply.blockers.iter().map(|b| b.code).collect();
        assert_eq!(
            codes,
            vec![
                SettlementBlockerCode::BillNotOwed,
                SettlementBlockerCode::AmountMissing,
                SettlementBlockerCode::OverBill,
                SettlementBlockerCode::BillRepeated,
                SettlementBlockerCode::OverPayment,
            ]
        );
        assert_eq!(reply.blockers[2].due_cents, Some(20_000));
        assert_eq!(reply.allocated_cents, 30_000 + 25_000);
    }

    #[test]
    fn a_fair_split_has_no_blockers() {
        let reply = compute(&request(50_000, &[("prc_old", 30_000), ("prc_new", 20_000)]), &bills()).unwrap();
        assert!(reply.blockers.is_empty());
        assert_eq!(reply.allocated_cents, 50_000);
    }

    #[test]
    fn out_of_range_amounts_are_refused() {
        assert!(compute(&request(-1, &[]), &bills()).is_err());
        assert!(compute(&request(1, &[("prc_old", MAX_CENTS + 1)]), &bills()).is_err());
    }
}
