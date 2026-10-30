use super::opening_key::opening_key;
use super::parse_day::parse_day;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::balances::{BalanceSubject, SetBalanceOpeningRequest};
use crate::modules::balances::model::BalanceOpeningDocument;
use crate::modules::balances::repository;
use crate::modules::{employees, suppliers};
use chrono::Utc;
use mongodb::bson::DateTime as BsonDateTime;

/// Sets, or changes, where a supplier balance (or a staff member's earlier advance) starts.
pub async fn set_opening(
    db: &Db,
    subject: BalanceSubject,
    subject_key: &str,
    set_by_name: String,
    body: SetBalanceOpeningRequest,
) -> AppResult<()> {
    tracked("balances.opening_set", async move {
        match subject {
            BalanceSubject::Supplier => {
                suppliers::service::get_supplier_by_key(db, subject_key).await?;
            }
            BalanceSubject::Staff => {
                employees::service::get_employee_by_key(db, subject_key).await?;
            }
        }
        let as_of = parse_day(&body.as_of_date)?;
        if as_of > Utc::now().date_naive() {
            return Err(AppError::validation("The date cannot be in the future."));
        }
        let as_of_date = as_of.format("%Y-%m-%d").to_string();
        let key = opening_key(subject_key);
        let now = BsonDateTime::now();
        match repository::find_opening(db, &key).await? {
            Some(mut existing) => {
                existing.opening_balance_cents = body.opening_balance_cents;
                existing.as_of_date = as_of_date;
                existing.set_by_name_snapshot = set_by_name;
                existing.version += 1;
                existing.updated_at = now;
                repository::save_opening(db, &existing).await?;
            }
            None => {
                repository::insert_opening(
                    db,
                    BalanceOpeningDocument {
                        id: None,
                        key,
                        subject_kind: subject,
                        subject_key: subject_key.to_string(),
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
        Ok(())
    })
    .await
}
