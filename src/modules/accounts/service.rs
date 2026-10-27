// Business rules for the money accounts: where each starts, and what each holds now, worked out
// from sales payments, income, expenses and the cash moved to and from the bank.

use chrono::{NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::prefixes,
        error::{AppError, AppResult},
        logging::domain::tracked,
    },
    domain::{
        accounts::{
            AccountActivity, AccountKind, AccountSummary, AccountsResponse,
            SetAccountOpeningRequest,
        },
        cash_categories::category_key,
        expenses::PaidFrom,
        incomes::ReceivedInto,
        sessions::CashMovementType,
    },
    modules::{
        accounts::{model::AccountOpeningDocument, repository},
        billing, cash_categories, expenses, incomes, sessions,
    },
};

/// The cash drop that takes cash to the bank.
const BANK_DEPOSIT: &str = "To bank deposit";
/// The add-cash that brings cash back from the bank.
const BANK_WITHDRAWAL: &str = "Bank withdrawal to drawer";

/// The accounts that start from an opening balance (everything but the drawer).
const TRACKED: [AccountKind; 3] = [AccountKind::Bank, AccountKind::Card, AccountKind::Online];

fn opening_key(kind: AccountKind) -> String {
    format!("{}_{}", prefixes::ACCOUNT_OPENING, kind.as_str())
}

/// What an account holds: where it started, plus what came in, less what went out.
fn balance_of(opening_cents: i64, a: &AccountActivity) -> i64 {
    opening_cents + a.sales_cents - a.refunds_cents + a.income_cents + a.deposits_cents
        - a.expenses_cents
        - a.withdrawals_cents
}

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

async fn summary(db: &Db, kind: AccountKind) -> AppResult<AccountSummary> {
    let opening = repository::find_by_key(db, &opening_key(kind)).await?;
    let activity = match &opening {
        Some(o) => Some(activity_since(db, kind, &o.as_of_date).await?),
        None => None,
    };
    Ok(summary_of(kind, opening.as_ref(), activity))
}

/// Every account with its balance: the drawer's cash across all terminals, then the bank, card and online accounts.
pub async fn list_accounts(db: &Db) -> AppResult<AccountsResponse> {
    let mut accounts = vec![AccountSummary {
        kind: AccountKind::Drawer,
        opening_balance_cents: None,
        as_of_date: None,
        balance_cents: Some(sessions::service::drawer_cash_total(db).await?),
        activity: None,
    }];
    for kind in TRACKED {
        accounts.push(summary(db, kind).await?);
    }
    Ok(AccountsResponse { accounts })
}

/// Sets, or changes, where the bank, card or online account started.
pub async fn set_opening(
    db: &Db,
    kind: AccountKind,
    set_by_name: String,
    body: SetAccountOpeningRequest,
) -> AppResult<AccountSummary> {
    tracked("accounts.opening_set", async move {
        if kind == AccountKind::Drawer {
            return Err(AppError::validation(
                "The drawer is counted from the shifts. Only the bank, card and online accounts start from an opening balance.",
            ));
        }
        let as_of = NaiveDate::parse_from_str(body.as_of_date.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::validation("Please pick a valid date."))?;
        if as_of > Utc::now().date_naive() {
            return Err(AppError::validation("The date cannot be in the future."));
        }
        let as_of_date = as_of.format("%Y-%m-%d").to_string();
        let key = opening_key(kind);
        let now = BsonDateTime::now();
        match repository::find_by_key(db, &key).await? {
            Some(mut existing) => {
                existing.opening_balance_cents = body.opening_balance_cents;
                existing.as_of_date = as_of_date;
                existing.set_by_name_snapshot = set_by_name;
                existing.version += 1;
                existing.updated_at = now;
                repository::save(db, &existing).await?;
            }
            None => {
                repository::insert(
                    db,
                    AccountOpeningDocument {
                        id: None,
                        key,
                        kind,
                        opening_balance_cents: body.opening_balance_cents,
                        as_of_date,
                        set_by_name_snapshot: set_by_name,
                        version: 1,
                        created_at: now,
                        updated_at: now,
                        updated_by_device: None,
                    },
                )
                .await?;
            }
        }
        summary(db, kind).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_balance_is_the_opening_plus_in_less_out() {
        let a = AccountActivity {
            sales_cents: 1000,
            refunds_cents: 100,
            income_cents: 50,
            expenses_cents: 300,
            deposits_cents: 700,
            withdrawals_cents: 200,
        };
        assert_eq!(
            balance_of(5000, &a),
            5000 + 1000 - 100 + 50 + 700 - 300 - 200
        );
        assert_eq!(balance_of(-250, &AccountActivity::default()), -250);
    }

    #[test]
    fn keys_are_fixed_per_account() {
        assert_eq!(opening_key(AccountKind::Bank), "aopen_bank");
    }
}
