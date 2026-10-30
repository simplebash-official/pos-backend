use super::calc_settlement_part_reply::CalcSettlementPartReply;
use super::calc_settlement_reply::CalcSettlementReply;
use super::calc_settlement_request::CalcSettlementRequest;
use super::max_cents::MAX_CENTS;
use super::open_bill::OpenBill;
use super::settlement_blocker::SettlementBlocker;
use super::settlement_blocker_code::SettlementBlockerCode;
use crate::core::error::{AppError, AppResult};

use std::collections::HashSet;

const MAX_SETTLEMENTS: usize = 200;

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
