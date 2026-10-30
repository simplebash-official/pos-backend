use crate::domain::users::Role;

/// `(tenant|user_id) -> (Some((is_active, role)) | None if deleted, read at)`.
pub(super) type AccountStatusCache =
    std::collections::HashMap<String, (Option<(bool, Role)>, std::time::Instant)>;

pub(super) static ACCOUNT_STATUS: std::sync::LazyLock<std::sync::Mutex<AccountStatusCache>> =
    std::sync::LazyLock::new(Default::default);
