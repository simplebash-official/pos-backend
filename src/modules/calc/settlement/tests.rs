use super::calc_settlement_part::CalcSettlementPart;
use super::calc_settlement_part_reply::CalcSettlementPartReply;
use super::calc_settlement_request::CalcSettlementRequest;
use super::compute::compute;
use super::max_cents::MAX_CENTS;
use super::open_bill::OpenBill;
use super::settlement_blocker_code::SettlementBlockerCode;

fn bills() -> Vec<OpenBill> {
    vec![
        OpenBill {
            key: "prc_old".into(),
            due_cents: 30_000,
        },
        OpenBill {
            key: "prc_mid".into(),
            due_cents: 50_000,
        },
        OpenBill {
            key: "prc_new".into(),
            due_cents: 20_000,
        },
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
    parts
        .iter()
        .map(|p| (p.receipt_key.as_str(), p.amount_cents))
        .collect()
}

#[test]
fn ticking_a_bill_takes_what_is_left_of_the_payment_or_the_whole_bill() {
    let reply = compute(&request(40_000, &[("prc_old", 30_000)]), &bills()).unwrap();
    assert_eq!(
        spread(&reply.if_ticked),
        [
            ("prc_old", 10_000),
            ("prc_mid", 10_000),
            ("prc_new", 10_000)
        ]
    );
    let none_left = compute(&request(30_000, &[("prc_old", 30_000)]), &bills()).unwrap();
    assert_eq!(spread(&none_left.if_ticked)[1], ("prc_mid", 50_000));
}

#[test]
fn a_payment_pays_the_oldest_bills_first() {
    let reply = compute(&request(60_000, &[]), &bills()).unwrap();
    assert_eq!(reply.owed_cents, 100_000);
    assert_eq!(
        spread(&reply.oldest_first),
        vec![("prc_old", 30_000), ("prc_mid", 30_000)]
    );
    let more = compute(&request(150_000, &[]), &bills()).unwrap();
    // Everything is paid; the rest stays unallocated.
    assert_eq!(spread(&more.oldest_first).len(), 3);
    assert_eq!(
        more.oldest_first
            .iter()
            .map(|p| p.amount_cents)
            .sum::<i64>(),
        100_000
    );
    assert_eq!(
        spread(&more.pay_everything),
        vec![
            ("prc_old", 30_000),
            ("prc_mid", 50_000),
            ("prc_new", 20_000)
        ]
    );
}

#[test]
fn nothing_is_spread_without_a_payment() {
    assert!(
        compute(&request(0, &[]), &bills())
            .unwrap()
            .oldest_first
            .is_empty()
    );
}

#[test]
fn a_split_is_checked_bill_by_bill() {
    let reply = compute(
        &request(
            40_000,
            &[
                ("prc_old", 30_000),
                ("prc_gone", 1_000),
                ("prc_mid", 0),
                ("prc_new", 25_000),
                ("prc_old", 5),
            ],
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
    let reply = compute(
        &request(50_000, &[("prc_old", 30_000), ("prc_new", 20_000)]),
        &bills(),
    )
    .unwrap();
    assert!(reply.blockers.is_empty());
    assert_eq!(reply.allocated_cents, 50_000);
}

#[test]
fn out_of_range_amounts_are_refused() {
    assert!(compute(&request(-1, &[]), &bills()).is_err());
    assert!(compute(&request(1, &[("prc_old", MAX_CENTS + 1)]), &bills()).is_err());
}
