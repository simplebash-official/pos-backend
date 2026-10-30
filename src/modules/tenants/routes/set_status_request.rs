use crate::core::error::{AppError, AppResult};

use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusRequest {
    /// True switches the shop's cloud access off; false switches it back on.
    pub suspended: bool,
    /// When the account was suspended (RFC 3339). Required with `suspended: true`, absent or null
    /// with `suspended: false`.
    pub suspended_at: Option<String>,
    /// Why (1-500 characters). Same presence rule as `suspended_at`; shown to the shop's staff.
    pub reason: Option<String>,
}

impl SetStatusRequest {
    /// The suspension to store, or `None` to reinstate; 400 when the fields do not match `suspended`.
    pub(super) fn suspension(&self) -> AppResult<Option<(mongodb::bson::DateTime, &str)>> {
        match (self.suspended, &self.suspended_at, &self.reason) {
            (false, None, None) => Ok(None),
            (false, _, _) => Err(AppError::validation(
                "suspendedAt and reason must be absent when suspended is false",
            )),
            (true, Some(at), Some(reason)) => {
                let reason = reason.trim();
                if reason.is_empty() || reason.chars().count() > 500 {
                    return Err(AppError::validation("reason must be 1-500 characters"));
                }
                let at = chrono::DateTime::parse_from_rfc3339(at)
                    .map_err(|_| AppError::validation("suspendedAt must be an RFC 3339 time"))?;
                Ok(Some((
                    mongodb::bson::DateTime::from_chrono(at.to_utc()),
                    reason,
                )))
            }
            (true, _, _) => Err(AppError::validation(
                "suspendedAt and reason are required when suspended is true",
            )),
        }
    }
}
