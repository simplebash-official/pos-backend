use super::compute::compute;
use super::shift_figures::ShiftFigures;

use crate::domain::sessions::SessionStatus;

fn shift(status: SessionStatus, cash: i64, variance: Option<i64>) -> ShiftFigures {
    ShiftFigures {
        status,
        cash_sales_cents: cash,
        cash_variance_cents: variance,
    }
}

#[test]
fn only_closed_shifts_count_towards_the_net_difference() {
    let reply = compute(&[
        shift(SessionStatus::Closed, 10_000, Some(-300)),
        shift(SessionStatus::Closed, 5_000, Some(100)),
        shift(SessionStatus::Open, 2_500, None),
        shift(SessionStatus::Reopened, 1_000, Some(900)),
    ]);
    assert_eq!(reply.shift_count, 4);
    assert_eq!(reply.running_count, 2);
    assert_eq!(reply.cash_sales_cents, 18_500);
    assert_eq!(reply.net_variance_cents, -200);
}
