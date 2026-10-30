use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Status of a cash register session / shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Open,
    Closing,
    Closed,
    Reopened,
}

impl SessionStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Open => "open",
            SessionStatus::Closing => "closing",
            SessionStatus::Closed => "closed",
            SessionStatus::Reopened => "reopened",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "open" => Some(SessionStatus::Open),
            "closing" => Some(SessionStatus::Closing),
            "closed" => Some(SessionStatus::Closed),
            "reopened" => Some(SessionStatus::Reopened),
            _ => None,
        }
    }
}
