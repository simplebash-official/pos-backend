// Business logic for the dashboard reminders feed — clamps the query
// parameters and assembles the `RemindersResponse` from the repository's
// merged list + headline counts.

use mongodb::Database;

use crate::{
    core::{error::AppResult, utils::calculate_pagination},
    domain::reports::{RemindersQuery, RemindersResponse},
    modules::reports::repository,
};

const DEFAULT_DUE_WITHIN_DAYS: u32 = 7;
const DEFAULT_LIMIT: u64 = 20;

pub(crate) async fn get_reminders(
    db: &Database,
    query: RemindersQuery,
) -> AppResult<RemindersResponse> {
    let due_within_days = query
        .due_within_days
        .unwrap_or(DEFAULT_DUE_WITHIN_DAYS)
        .clamp(1, 30);
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, DEFAULT_LIMIT, 100);

    let raw = repository::reminders::list_reminders(db, due_within_days, page, limit, skip).await?;

    Ok(RemindersResponse {
        reminders: raw.entries,
        credit_overdue_count: raw.credit_overdue_count,
        credit_overdue_amount_cents: raw.credit_overdue_amount_cents,
        credit_due_soon_count: raw.credit_due_soon_count,
        credit_due_soon_amount_cents: raw.credit_due_soon_amount_cents,
        jobs_overdue_count: raw.jobs_overdue_count,
        jobs_due_soon_count: raw.jobs_due_soon_count,
        due_within_days,
        total: raw.total,
        page: raw.page,
        limit: raw.limit,
        total_pages: raw.total_pages,
    })
}
