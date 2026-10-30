use super::advance_summary::advance_summary;
use super::advances_given::advances_given;
use super::given::Given;
use super::opening_key::opening_key;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::balances::{AdvanceGiven, AdvanceRecovery, StaffAdvanceDetail};
use crate::modules::balances::model::AdvanceRecoveryDocument;
use crate::modules::balances::repository;
use crate::modules::{cash_categories, employees};

fn recovery_dto(r: &AdvanceRecoveryDocument) -> AdvanceRecovery {
    AdvanceRecovery {
        key: r.key.clone(),
        employee_key: r.employee_key.clone(),
        amount_cents: r.amount_cents,
        recovered_on: r.recovered_on.clone(),
        note: r.note.clone(),
        recorded_by_name_snapshot: r.recorded_by_name_snapshot.clone(),
        created_at: r.created_at.to_chrono(),
    }
}

/// One staff member's advances handed out and recovered.
pub async fn get_staff_advance(db: &Db, employee_key: &str) -> AppResult<StaffAdvanceDetail> {
    let employee = employees::service::get_employee_by_key(db, employee_key).await?;
    let all_given = advances_given(db).await?;
    let given: Vec<&Given> = all_given
        .iter()
        .filter(|g| g.employee_key == employee_key)
        .collect();
    let all_recoveries = repository::list_recoveries(db).await?;
    let recoveries: Vec<&AdvanceRecoveryDocument> = all_recoveries
        .iter()
        .filter(|r| r.employee_key == employee_key)
        .collect();
    let opening = repository::find_opening(db, &opening_key(employee_key)).await?;
    let summary = advance_summary(
        employee_key,
        employee.name,
        opening.as_ref(),
        &given,
        &recoveries,
    );

    let since = summary.as_of_date.clone();
    let since = since.as_deref();
    let names = cash_categories::service::display_names(
        db,
        &given
            .iter()
            .map(|g| g.category_key.clone())
            .collect::<Vec<_>>(),
    )
    .await?;
    let mut entries: Vec<&Given> = given
        .iter()
        .copied()
        .filter(|g| since.is_none_or(|s| g.day.as_str() >= s))
        .collect();
    entries.sort_by(|a, b| b.day.cmp(&a.day).then(b.number.cmp(&a.number)));
    Ok(StaffAdvanceDetail {
        summary,
        given: entries
            .into_iter()
            .map(|g| AdvanceGiven {
                number: g.number.clone(),
                date: g.day.clone(),
                amount_cents: g.cents,
                category_name: names[&g.category_key].clone(),
            })
            .collect(),
        recoveries: recoveries
            .into_iter()
            .filter(|r| since.is_none_or(|s| r.recovered_on.as_str() >= s))
            .rev()
            .map(recovery_dto)
            .collect(),
    })
}
