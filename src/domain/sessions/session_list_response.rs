use super::register_session::RegisterSession;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated list of register sessions.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    pub sessions: Vec<RegisterSession>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
