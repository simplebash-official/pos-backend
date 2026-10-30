use super::balance_of::balance_of;
use super::opening_key::opening_key;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::accounts::{AccountActivity, AccountKind, AccountSummary};
use crate::domain::cash_categories::category_key;
use crate::domain::expenses::PaidFrom;
use crate::domain::incomes::ReceivedInto;
use crate::domain::sessions::CashMovementType;
use crate::modules::accounts::model::AccountOpeningDocument;
use crate::modules::accounts::repository;
use crate::modules::{billing, cash_categories, expenses, incomes, sessions};

/// The cash drop that takes cash to the bank.
const BANK_DEPOSIT: &str = "To bank deposit";

/// The add-cash that brings cash back from the bank.
const BANK_WITHDRAWAL: &str = "Bank withdrawal to drawer";

fn summary_of(
    kind: AccountKind,
    opening: Option<&AccountOpeningDocument>,
    activity: Option<AccountActivity>,
) -> AccountSummary {
    match (opening, activity) {
        (Some(o), Some(a)) => AccountSummary {
            kind,
            opening_balance_cents: Some(o.opening_balance_cents),
            as_of_date: Some(o.as_of_date.clone()),
            balance_cents: Some(balance_of(o.opening_balance_cents, &a)),
            activity: Some(a),
        },
        _ => AccountSummary {
            kind,
            opening_balance_cents: None,
            as_of_date: None,
            balance_cents: None,
            activity: None,
        },
    }
}

/// Everything that moved through one account on or after `since` (`YYYY-MM-DD`).
async fn activity_since(db: &Db, kind: AccountKind, since: &str) -> AppResult<AccountActivity> {
    let mut activity = AccountActivity::default();
    let paid_from = match kind {
        AccountKind::Bank => PaidFrom::Bank,
        AccountKind::Card => PaidFrom::Card,
        AccountKind::Online => PaidFrom::Online,
        AccountKind::Drawer => unreachable!("the drawer has no opening balance"),
    };
    let received_into = match kind {
        AccountKind::Bank => ReceivedInto::Bank,
        AccountKind::Card => ReceivedInto::Card,
        AccountKind::Online => ReceivedInto::Online,
        AccountKind::Drawer => unreachable!("the drawer has no opening balance"),
    };
    activity.expenses_cents = expenses::service::account_payments_since(db, since)
        .await?
        .into_iter()
        .filter(|(from, _)| *from == paid_from)
        .map(|(_, cents)| cents)
        .sum();
    activity.income_cents = incomes::service::account_receipts_since(db, since)
        .await?
        .into_iter()
        .filter(|(into, _)| *into == received_into)
        .map(|(_, cents)| cents)
        .sum();
    match kind {
        AccountKind::Card | AccountKind::Online => {
            for (method, received, refunded) in
                billing::service::payments::totals_by_method_since(db, since).await?
            {
                if method == kind.as_str() {
                    activity.sales_cents += received;
                    activity.refunds_cents += refunded;
                }
            }
        }
        AccountKind::Bank => {
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
            let (deposited, withdrawn) =
                sessions::service::bank_cash_moves(db, since, &deposit, &withdrawal).await?;
            activity.deposits_cents = deposited;
            activity.withdrawals_cents = withdrawn;
        }
        AccountKind::Drawer => {}
    }
    Ok(activity)
}

pub(super) async fn summary(db: &Db, kind: AccountKind) -> AppResult<AccountSummary> {
    let opening = repository::find_by_key(db, &opening_key(kind)).await?;
    let activity = match &opening {
        Some(o) => Some(activity_since(db, kind, &o.as_of_date).await?),
        None => None,
    };
    Ok(summary_of(kind, opening.as_ref(), activity))
}
