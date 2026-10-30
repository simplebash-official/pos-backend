use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The three fixed roles this system supports. Each maps to a hardcoded
/// default permission set (`core::constants::roles::default_permissions`) —
/// there are no custom/DB-defined roles. Serializes to a lowercase string
/// (`"admin"`/`"manager"`/`"staff"`) so it drops straight into a JWT claim
/// and matches the string `core::middleware::auth::AdminUser` already
/// compared against before this type existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Manager,
    Staff,
}

impl Role {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Manager => "manager",
            Role::Staff => "staff",
        }
    }

    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "manager" => Some(Role::Manager),
            "staff" => Some(Role::Staff),
            _ => None,
        }
    }
}

impl std::str::FromStr for Role {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Role::from_str(s).ok_or(())
    }
}
