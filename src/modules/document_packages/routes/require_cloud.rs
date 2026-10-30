use crate::app::AppState;
use crate::core::config::TenantMode;
use crate::core::error::{AppError, AppResult};

pub(super) fn require_cloud(state: &AppState) -> AppResult<()> {
    if state.config.tenant_mode == TenantMode::Multi {
        Ok(())
    } else {
        Err(AppError::not_found("Not found"))
    }
}
