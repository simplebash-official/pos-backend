use crate::domain::users::Role;

/// The roles a caller of `caller_role` may create/view/edit/delete through
/// this module's endpoints: Admin manages Manager and Staff; Manager
/// manages Staff only. An Admin account is never manageable through this
/// API by anyone — not even by another Admin — since this is a
/// single-shop POS deployment that only ever has one Admin, bootstrapped
/// once via `src/bin/seed_admin.rs` (see `create_user`'s single-Admin
/// guard below). Staff never reaches these functions at all (no
/// `users:manage`/`users:manage:staff` permission to get past the route
/// gate in the first place).
pub(super) fn manageable_roles(caller_role: Role) -> &'static [Role] {
    match caller_role {
        Role::Admin => &[Role::Manager, Role::Staff],
        Role::Manager => &[Role::Staff],
        Role::Staff => &[],
    }
}
