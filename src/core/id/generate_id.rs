use nanoid::nanoid;

/// Generates a unique key prefixed with a model identifier and separated by an underscore.
/// Format: `<prefix>_<nanoid>` (e.g., `prod_x7K9mP2...`, `cat_B8nL1q...`).
/// Invariant: Server-minted keys must NEVER begin with `local_` (reserved for provisional client offline IDs).
pub fn generate_id(prefix: &str) -> String {
    assert!(!prefix.is_empty(), "Key prefix must not be empty");
    assert!(
        !prefix.starts_with("local_") && prefix != "local",
        "Server must never issue keys starting with 'local_'"
    );
    format!("{}_{}", prefix, nanoid!(16))
}
