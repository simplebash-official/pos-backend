use crate::core::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
/// The amount a payment settles against one bill.
pub struct CalcSettlementPart {
    pub receipt_key: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementPartReply {
    pub receipt_key: String,
    pub amount_cents: i64,
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

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementRequest {
    pub supplier_key: String,
    /// What is being paid; 0 until typed.
    pub payment_cents: i64,
    /// How the person split the payment over bills.
    pub settlements: Vec<CalcSettlementPart>,
}

const MAX_SETTLEMENTS: usize = 200;

/// Splits a payment across the open bills it settles.
pub fn compute(
    request: &CalcSettlementRequest,
    open: &[OpenBill],
) -> AppResult<CalcSettlementReply> {
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
            amount_cents: if room > 0 {
                room.min(bill.due_cents)
            } else {
                bill.due_cents
            },
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

pub(super) const MAX_CENTS: i64 = 10_000_000_000_000;

/// An open bill, oldest first.
#[derive(Debug)]
pub struct OpenBill {
    pub key: String,
    pub due_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
/// A bill that cannot be settled and why.
pub struct SettlementBlocker {
    pub code: SettlementBlockerCode,
    /// The bill, for the bill blockers.
    pub receipt_key: Option<String>,
    /// What the bill still owes, for `over_bill`.
    pub due_cents: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
/// The reason a bill cannot be settled.
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
