use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::domain::attachments::AttachmentOwner;

/// A photo can be seen (or added) only by someone who can see (or record) the record it is on.
pub(super) fn require_owner_access(
    user: &CurrentUser,
    kind: AttachmentOwner,
    write: bool,
) -> AppResult<()> {
    let (view, record) = match kind {
        AttachmentOwner::CashMovement => (perm::CASH_MOVEMENTS_VIEW, perm::CASH_MOVEMENTS_CREATE),
        AttachmentOwner::Expense => (perm::EXPENSES_VIEW, perm::EXPENSES_MANAGE),
        AttachmentOwner::Income => (perm::INCOME_VIEW, perm::INCOME_MANAGE),
    };
    user.require_permission(if write {
        perm::ATTACHMENTS_CREATE
    } else {
        perm::ATTACHMENTS_VIEW
    })?;
    user.require_permission(if write { record } else { view })
}
