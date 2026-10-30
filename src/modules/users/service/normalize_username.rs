pub(super) fn normalize_username(username: &str) -> String {
    username.trim().to_lowercase()
}
