use crate::core::constants::roles::default_permissions;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::domain::notifications::{AppAudience, NotificationAudience};
use crate::domain::users::Role;
use crate::modules::notifications::repository::Viewer;

/// Who is reading, for the visibility rule: everyone's notifications, those for a permission the
/// caller's token holds, and those addressed to the caller.
pub(super) fn viewer_of(caller: &CurrentUser) -> Viewer<'_> {
    Viewer {
        user_key: &caller.user_id,
        permissions: &caller.permissions,
    }
}

/// The audience of an alert the app raises: everyone, the holders of a permission the caller
/// holds (so nobody can reach people outside their own role), or the caller alone. Never another
/// single user, so nobody can message a colleague through it.
pub(super) fn app_audience(
    caller: &CurrentUser,
    audience: AppAudience,
) -> AppResult<NotificationAudience> {
    match audience {
        AppAudience::Everyone => Ok(NotificationAudience::Everyone),
        AppAudience::Permission { permission } => {
            let permission = permission.trim().to_string();
            if caller.permissions.contains(&permission) {
                Ok(NotificationAudience::Permission { permission })
            } else {
                Err(AppError::forbidden(
                    "You can only notify people who share one of your permissions",
                ))
            }
        }
        AppAudience::Me => Ok(NotificationAudience::User {
            user_key: caller.user_id.clone(),
        }),
    }
}

/// A service may address everyone, or the holders of a permission that some role grants.
pub(super) fn check_service_audience(audience: &NotificationAudience) -> AppResult<()> {
    match audience {
        NotificationAudience::Everyone => Ok(()),
        NotificationAudience::Permission { permission } => {
            let known = [Role::Admin, Role::Manager, Role::Staff]
                .into_iter()
                .any(|role| default_permissions(role).contains(&permission.trim()));
            if known {
                Ok(())
            } else {
                Err(AppError::validation(format!(
                    "Unknown permission '{permission}'"
                )))
            }
        }
        NotificationAudience::User { .. } => Err(AppError::validation(
            "A service notice goes to everyone or to the holders of a permission",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caller(permissions: &[&str]) -> CurrentUser {
        CurrentUser {
            user_id: "usr_1".to_string(),
            role: Some(Role::Staff),
            permissions: permissions.iter().map(|p| p.to_string()).collect(),
            tenant_id: None,
            scope: None,
            device_id: None,
            email: None,
            name: None,
        }
    }

    #[test]
    fn the_app_addresses_only_permissions_it_holds_or_itself() {
        let staff = caller(&["inventory:read"]);
        assert_eq!(
            app_audience(&staff, AppAudience::Everyone).expect("allowed"),
            NotificationAudience::Everyone
        );
        let held = AppAudience::Permission {
            permission: " inventory:read ".to_string(),
        };
        assert_eq!(
            app_audience(&staff, held).expect("allowed"),
            NotificationAudience::Permission {
                permission: "inventory:read".to_string()
            }
        );
        let foreign = AppAudience::Permission {
            permission: "expenses:view".to_string(),
        };
        assert!(app_audience(&staff, foreign).is_err());
        assert_eq!(
            app_audience(&staff, AppAudience::Me).expect("allowed"),
            NotificationAudience::User {
                user_key: "usr_1".to_string()
            }
        );
    }

    #[test]
    fn a_service_addresses_everyone_or_a_known_permission() {
        assert!(check_service_audience(&NotificationAudience::Everyone).is_ok());
        let known = NotificationAudience::Permission {
            permission: "reports:view".to_string(),
        };
        assert!(check_service_audience(&known).is_ok());
        let typo = NotificationAudience::Permission {
            permission: "report:view".to_string(),
        };
        assert!(check_service_audience(&typo).is_err());
        let user = NotificationAudience::User {
            user_key: "usr_1".to_string(),
        };
        assert!(check_service_audience(&user).is_err());
    }
}
