use crate::domain::users::Role;

/// Maps token scopes onto the local role model. Platform tokens carry no
/// `role`/`permissions` claims, so this is the single place that decides what
/// a scope may do:
///   - `owner`  (the business owner's account token, e.g. on the web)  -> Admin
///   - `account` (an interactive identity login; identity only puts a `tid`
///     on it for a shop the account owns, so `verify_platform_token`
///     insists on one)                                                  -> Admin
///   - `device` (a linked desktop installation acting for its shop)     -> Admin
///   - `staff`                                                          -> Manager
///
/// The highest matching scope wins; a token with no recognised scope is
/// rejected rather than given a default role.
#[must_use]
pub fn role_for_scope(scope: &str) -> Option<Role> {
    let mut best = None;
    for s in scope.split_whitespace() {
        match s {
            "owner" | "device" | "account" => return Some(Role::Admin),
            "staff" => best = Some(Role::Manager),
            _ => {}
        }
    }
    best
}
