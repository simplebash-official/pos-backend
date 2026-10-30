use super::clean::clean;
use super::get_income::get_income;
use super::parse_day::parse_day;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::logging::domain::tracked;
use crate::domain::incomes::{CreateIncomeRequest, IncomeRow, ReceivedInto};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::modules::incomes::model::IncomeDocument;
use crate::modules::incomes::repository;
use crate::modules::{cash_categories, payees, sequences};
use mongodb::bson::DateTime as BsonDateTime;

const TEXT_MAX: usize = 200;

const REFERENCE_MAX: usize = 60;

const NAME_MAX: usize = 60;

/// Enters money received into the bank, a card or online. Cash put into the drawer is an
/// add-cash movement, not an entry here.
pub async fn create_income(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateIncomeRequest,
    device_id: Option<String>,
) -> AppResult<IncomeRow> {
    tracked("incomes.created", async move {
        if body.received_into == ReceivedInto::Drawer {
            return Err(AppError::validation(
                "Cash put into the drawer is recorded as a cash movement. It appears here by itself.",
            ));
        }
        if body.amount_cents <= 0 {
            return Err(AppError::validation("Please enter an amount greater than zero."));
        }
        parse_day("income date", &body.income_date)?;
        let category = cash_categories::service::category_for_income(db, &body.category_key).await?;

        let typed = clean("The name", body.payee_name.as_deref(), NAME_MAX)?;
        let (payee_key, payee_name) = match (&body.payee_key, typed) {
            (Some(key), _) => {
                let resolved = payees::service::resolve_payee(db, key).await?;
                (Some(key.clone()), Some(resolved.name))
            }
            (None, Some(name)) => {
                let saved = payees::service::ensure_other_payee(db, &name).await?;
                (Some(saved.key), Some(saved.name))
            }
            (None, None) => (None, None),
        };

        let reservation = sequences::service::reserve_sequence(
            db,
            "income".to_string(),
            ReserveSequenceRequest { block_size: Some(1), device_id },
        )
        .await?;
        let income_number = format!(
            "{}{:0width$}",
            reservation.prefix,
            reservation.start,
            width = reservation.padding
        );

        let now = BsonDateTime::now();
        let income = repository::insert_income(
            db,
            IncomeDocument {
                id: None,
                key: generate_id(prefixes::INCOME),
                income_number,
                income_date: body.income_date.trim().to_string(),
                category_key: category.key,
                payee_key,
                payee_name,
                amount_cents: body.amount_cents,
                received_into: body.received_into,
                reference_no: clean("The reference", body.reference_no.as_deref(), REFERENCE_MAX)?,
                note: clean("The note", body.note.as_deref(), TEXT_MAX)?,
                recorded_by_user_id,
                recorded_by_name_snapshot: recorded_by_name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        get_income(db, &income.key).await
    })
    .await
}
