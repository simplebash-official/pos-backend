use nanoid::nanoid;

/// Generates a unique key prefixed with a model identifier and separated by an underscore.
/// Format: `<prefix>_<nanoid>` (e.g., `prod_x7K9mP2...`, `cat_B8nL1q...`).
pub fn generate_id(prefix: &str) -> String {
    format!("{}_{}", prefix, nanoid!(16))
}
