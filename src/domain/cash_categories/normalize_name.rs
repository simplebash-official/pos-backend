/// Search/uniqueness form of a name: lowercase with all whitespace removed, so
/// "Tea  and Refreshments" and "tea and refreshments" are the same category.
pub fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}
