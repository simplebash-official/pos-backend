use super::summary::summary;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::accounts::{AccountKind, AccountSummary, AccountsResponse};
use crate::modules::sessions;

/// The accounts that start from an opening balance (everything but the drawer).
const TRACKED: [AccountKind; 3] = [AccountKind::Bank, AccountKind::Card, AccountKind::Online];

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
