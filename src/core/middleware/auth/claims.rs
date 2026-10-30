use crate::domain::users::Role;

use serde::{Deserialize, Serialize};

/// Claims embedded in the JWT issued at login (`modules::auth::service::login`)
/// and verified on every authenticated request. `role` is optional so
/// tokens without it still decode; `permissions` defaults to empty for the
/// same reason — both matter only to extractors/checks that require them
/// (`AdminUser`, `CurrentUser::require_permission`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    #[serde(default)]
    pub role: Option<Role>,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Tenant the token was issued for. Absent on single-shop deployments;
    /// required by `CurrentUser` when `TENANT_MODE=multi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
    /// Space-separated scopes (`device`, `owner`, ...). Set only on tokens
    /// minted for a registered sync device; ordinary logins carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Registered device id, present on device tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,
    /// Display name of the user. Present on local login tokens and identity tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
