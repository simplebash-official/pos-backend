use super::suspension_info::SuspensionInfo;

use crate::core::constants::codes;
use crate::core::error::AppError;

/// The refusal every request of a suspended shop gets; `details` feeds the suspended page.
pub(crate) fn suspended_error(info: &SuspensionInfo) -> AppError {
    AppError::custom_with_details(
        axum::http::StatusCode::FORBIDDEN,
        codes::ACCOUNT_SUSPENDED,
        "This shop's cloud access is suspended. The desktop app keeps working offline; contact SimpleBash to restore access.",
        serde_json::to_value(info).expect("SuspensionInfo serialises"),
    )
}
