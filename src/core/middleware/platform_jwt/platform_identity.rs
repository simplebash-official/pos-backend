use crate::domain::users::Role;

/// What a verified platform token grants inside this backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformIdentity {
    pub sub: String,
    pub tid: Option<String>,
    pub role: Role,
    pub permissions: Vec<String>,
    /// Raw space-separated scope claim (kept so routes can require `device`).
    pub scope: String,
    /// Device id (`did`) for device tokens.
    pub device_id: Option<String>,
    /// Profile of the signed-in account, when the token carries it.
    pub email: Option<String>,
    pub name: Option<String>,
}
