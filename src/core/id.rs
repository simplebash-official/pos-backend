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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_id_creates_prefixed_key() {
        let key = generate_id("prod");
        assert!(key.starts_with("prod_"));
        assert_eq!(key.len(), 5 + 16);
    }

    #[test]
    #[should_panic(expected = "Server must never issue keys starting with 'local_'")]
    fn generate_id_panics_on_local_prefix() {
        generate_id("local");
    }

    #[test]
    #[should_panic(expected = "Server must never issue keys starting with 'local_'")]
    fn generate_id_panics_on_local_underscore_prefix() {
        generate_id("local_product");
    }
}
