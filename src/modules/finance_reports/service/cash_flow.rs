use super::day::day;
use super::parse_range::parse_range;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::accounts::AccountKind;
use crate::domain::cash_categories::category_key;
use crate::domain::expenses::PaidFrom;
use crate::domain::finance_reports::{AccountCashFlow, CashFlowLine, CashFlowResponse};
use crate::domain::incomes::ReceivedInto;
use crate::domain::sessions::CashMovementType;
use crate::modules::{billing, cash_categories, expenses, incomes, sessions};

/// The cash drop that takes cash to the bank, and the add-cash that brings it back (see the accounts).
const BANK_DEPOSIT: &str = "To bank deposit";

const BANK_WITHDRAWAL: &str = "Bank withdrawal to drawer";

/// A cumulative figure between two days: what it was from `from` on, less what it was from the day after `to` on.
async fn between<T, F, Fut>(from: &str, after_to: &str, read: F) -> AppResult<(T, T)>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = AppResult<T>>,
{
    Ok((
        read(from.to_string()).await?,
        read(after_to.to_string()).await?,
    ))
}

fn line(code: &str, in_cents: i64, out_cents: i64) -> CashFlowLine {
    CashFlowLine {
        code: code.to_string(),
        in_cents,
        out_cents,
    }
}

fn flow(kind: AccountKind, lines: Vec<CashFlowLine>) -> AccountCashFlow {
    let lines: Vec<CashFlowLine> = lines
        .into_iter()
        .filter(|l| l.in_cents != 0 || l.out_cents != 0)
        .collect();
    let (in_cents, out_cents) = (
        lines.iter().map(|l| l.in_cents).sum::<i64>(),
        lines.iter().map(|l| l.out_cents).sum::<i64>(),
    );
    AccountCashFlow {
        kind,
        in_cents,
        out_cents,
        net_cents: in_cents - out_cents,
        lines,
    }
}

/// Money in and out of each account over a period, with the reasons. A cash drop to the bank is money out of the
/// drawer and money into the bank, so it shows on both. Card and online sales stay in their own accounts (the
/// shop's own settlement into the bank is not recorded).
pub async fn cash_flow(db: &Db, from: &str, to: &str) -> AppResult<CashFlowResponse> {
    let (start, end) = parse_range(from, to)?;
    let after_end = day(end
        .succ_opt()
        .ok_or_else(|| AppError::validation("Please pick a valid end date."))?);
    let from_day = day(start);

    // Sales and refunds by method.
    let (a, b) = between(&from_day, &after_end, |d| async move {
        billing::service::payments::totals_by_method_since(db, &d).await
    })
    .await?;
    let sales = |method: &str| -> (i64, i64) {
        let pick = |rows: &[(String, i64, i64)]| {
            rows.iter()
                .filter(|(m, _, _)| m == method)
                .fold((0, 0), |acc, (_, r, f)| (acc.0 + r, acc.1 + f))
        };
        let (x, y) = (pick(&a), pick(&b));
        (x.0 - y.0, x.1 - y.1)
    };

    // Ledger payments and receipts by account.
    let (pa, pb) = between(&from_day, &after_end, |d| async move {
        expenses::service::account_payments_since(db, &d).await
    })
    .await?;
    let paid = |from_account: PaidFrom| -> i64 {
        let sum = |rows: &[(PaidFrom, i64)]| {
            rows.iter()
                .filter(|(f, _)| *f == from_account)
                .map(|(_, c)| c)
                .sum::<i64>()
        };
        sum(&pa) - sum(&pb)
    };
    let (ra, rb) = between(&from_day, &after_end, |d| async move {
        incomes::service::account_receipts_since(db, &d).await
    })
    .await?;
    let received = |into: ReceivedInto| -> i64 {
        let sum = |rows: &[(ReceivedInto, i64)]| {
            rows.iter()
                .filter(|(i, _)| *i == into)
                .map(|(_, c)| c)
                .sum::<i64>()
        };
        sum(&ra) - sum(&rb)
    };

    // The drawer's own movements, and the bank deposits among them.
    let deposit = cash_categories::service::keys_including_merged(
        db,
        &category_key(CashMovementType::CashDrop, BANK_DEPOSIT),
    )
    .await?;
    let withdrawal = cash_categories::service::keys_including_merged(
        db,
        &category_key(CashMovementType::AddCash, BANK_WITHDRAWAL),
    )
    .await?;
    let (mut cash_added, mut paid_out, mut owner, mut to_bank_or_safe) = (0, 0, 0, 0);
    let (mut deposited, mut withdrawn) = (0, 0);
    for m in sessions::service::movements_between(db, &from_day, &day(end)).await? {
        match m.movement_type {
            CashMovementType::AddCash => {
                cash_added += m.amount_cents;
                if withdrawal.contains(&m.category_key) {
                    withdrawn += m.amount_cents;
                }
            }
            CashMovementType::OwnerWithdrawal => owner += m.amount_cents,
            CashMovementType::CashDrop => {
                to_bank_or_safe += m.amount_cents;
                if deposit.contains(&m.category_key) {
                    deposited += m.amount_cents;
                }
            }
            _ => paid_out += m.amount_cents,
        }
    }

    let (cash_sales, cash_refunds) = sales("cash");
    let (card_sales, card_refunds) = sales("card");
    let (online_sales, online_refunds) = sales("online");
    Ok(CashFlowResponse {
        from: from_day,
        to: day(end),
        accounts: vec![
            flow(
                AccountKind::Drawer,
                vec![
                    line("sales", cash_sales, 0),
                    line("refunds", 0, cash_refunds),
                    line("cash_added", cash_added, 0),
                    line("paid_out", 0, paid_out),
                    line("owner_withdrawals", 0, owner),
                    line("cash_to_bank_or_safe", 0, to_bank_or_safe),
                ],
            ),
            flow(
                AccountKind::Bank,
                vec![
                    line("bank_deposits", deposited, 0),
                    line("other_income", received(ReceivedInto::Bank), 0),
                    line("expense_payments", 0, paid(PaidFrom::Bank)),
                    line("bank_withdrawals", 0, withdrawn),
                ],
            ),
            flow(
                AccountKind::Card,
                vec![
                    line("sales", card_sales, 0),
                    line("refunds", 0, card_refunds),
                    line("other_income", received(ReceivedInto::Card), 0),
                    line("expense_payments", 0, paid(PaidFrom::Card)),
                ],
            ),
            flow(
                AccountKind::Online,
                vec![
                    line("sales", online_sales, 0),
                    line("refunds", 0, online_refunds),
                    line("other_income", received(ReceivedInto::Online), 0),
                    line("expense_payments", 0, paid(PaidFrom::Online)),
                ],
            ),
        ],
    })
}
