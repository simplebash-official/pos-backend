use crate::core::error::{AppError, AppResult};
use crate::domain::expenses::PaidFrom;

const REMIND_MAX_DAYS: i64 = 30;

pub(super) fn check_schedule(
    amount_cents: i64,
    due_day: i64,
    remind_days_before: i64,
    paid_from: PaidFrom,
) -> AppResult<()> {
    if amount_cents <= 0 {
        return Err(AppError::validation(
            "Please enter an amount greater than zero.",
        ));
    }
    if !(1..=31).contains(&due_day) {
        return Err(AppError::validation(
            "The due day must be between 1 and 31.",
        ));
    }
    if !(0..=REMIND_MAX_DAYS).contains(&remind_days_before) {
        return Err(AppError::validation(format!(
            "Reminders can start 0 to {REMIND_MAX_DAYS} days before the due day."
        )));
    }
    if paid_from == PaidFrom::Unpaid {
        return Err(AppError::validation("Pick where it is usually paid from."));
    }
    Ok(())
}
