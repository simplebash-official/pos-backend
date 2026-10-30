use super::period_query::PeriodQuery;
use crate::core::error::{AppError, AppResult};

pub(super) fn period(query: PeriodQuery) -> AppResult<(String, String)> {
    match (query.from, query.to) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::validation(
            "Please pick the first and the last day of the report.",
        )),
    }
}
