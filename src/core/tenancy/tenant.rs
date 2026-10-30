use super::deny_tenant::DENY_TENANT;
use crate::core::error::{AppError, AppResult};
use mongodb::bson::Bson;

use std::sync::Arc;

/// Which data a database handle may see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tenant {
    /// No tenant filtering: single-shop deployments (desktop, self-hosted) and
    /// platform-level collections. Behaves exactly like the raw driver.
    Global,
    /// Multi-tenant cloud: every operation is confined to this tenant id.
    Id(Arc<str>),
    /// Multi-tenant cloud, no tenant known (unauthenticated or out-of-request
    /// work): matches nothing, so a missed scope returns no data.
    Deny,
}

impl Tenant {
    pub fn id(id: impl AsRef<str>) -> AppResult<Tenant> {
        let id = id.as_ref().trim();
        if id.is_empty() {
            return Err(AppError::internal("tenant id must not be empty"));
        }
        Ok(Tenant::Id(Arc::from(id)))
    }

    #[must_use]
    pub fn is_scoped(&self) -> bool {
        !matches!(self, Tenant::Global)
    }

    pub(super) fn value(&self) -> Option<Bson> {
        match self {
            Tenant::Global => None,
            Tenant::Id(id) => Some(Bson::String(id.to_string())),
            Tenant::Deny => Some(Bson::String(DENY_TENANT.to_string())),
        }
    }
}
