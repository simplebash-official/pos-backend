use super::secrets_match::secrets_match;
use crate::app::AppState;
use crate::core::config::TenantMode;

use crate::core::error::{AppError, AppResult};

use axum::http::HeaderMap;

const PROVISION_SECRET_HEADER: &str = "x-provision-secret";

/// The endpoint only exists in a multi-tenant deployment that configured a
/// secret; anywhere else it answers 404 as if it were not routed at all.
pub(super) fn authorize(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    let expected = match (
        state.config.tenant_mode,
        state.config.provision_secret.as_deref(),
    ) {
        (TenantMode::Multi, Some(secret)) => secret,
        _ => return Err(AppError::not_found("Not found")),
    };
    let provided = headers
        .get(PROVISION_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if secrets_match(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(AppError::unauthorized("Invalid provisioning secret"))
    }
}
