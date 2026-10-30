use crate::core::tenancy::Tenant;
use crate::domain::users::Role;

/// A bearer token that passed signature and expiry checks, from either issuer.
/// It is the ONE place the tenant of a request is derived (`tenant`), used by
/// both the `CurrentUser` extractor and the tenant middleware, so the tenant a
/// handler is authorised as and the tenant its queries are scoped to cannot
/// disagree.
#[derive(Debug, Clone)]
pub struct VerifiedToken {
    pub user_id: String,
    pub role: Option<Role>,
    pub permissions: Vec<String>,
    /// `None` when the token names no (or an empty) tenant.
    pub tenant: Option<Tenant>,
    pub scope: Option<String>,
    pub device_id: Option<String>,
    /// Profile claims of an identity token (`None` for local tokens).
    pub email: Option<String>,
    pub name: Option<String>,
}

impl VerifiedToken {
    pub(super) fn new(
        user_id: String,
        role: Option<Role>,
        permissions: Vec<String>,
        tid: Option<String>,
    ) -> Self {
        Self {
            user_id,
            role,
            permissions,
            tenant: tid.and_then(|t| Tenant::id(t).ok()),
            scope: None,
            device_id: None,
            email: None,
            name: None,
        }
    }

    #[must_use]
    pub fn tenant_id(&self) -> Option<String> {
        match &self.tenant {
            Some(Tenant::Id(id)) => Some(id.to_string()),
            _ => None,
        }
    }
}
