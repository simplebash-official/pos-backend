use super::account_status::ACCOUNT_STATUS;

/// Drops the cached status for `user_id` (in every tenant), so the next
/// request with that user's token re-reads the account. Called by the users
/// service after it deactivates, deletes or re-roles an account.
pub fn invalidate_account_status(user_id: &str) {
    let suffix = format!("|{user_id}");
    ACCOUNT_STATUS
        .lock()
        .expect("account status cache mutex poisoned")
        .retain(|key, _| !key.ends_with(&suffix));
}
