use super::opening_key::opening_key;
use super::summary::summary;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::accounts::{AccountKind, AccountSummary, SetAccountOpeningRequest};
use crate::modules::accounts::model::AccountOpeningDocument;
use crate::modules::accounts::repository;
use chrono::{NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;

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
