/// Escapes special regex characters in a search term and strips NUL bytes
/// so it can be safely used in a MongoDB regex query.
#[must_use]
pub fn regex_escape(input: &str) -> String {
    let sanitized = input.replace('\0', "");
    let mut escaped = String::with_capacity(sanitized.len());
    for c in sanitized.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}
