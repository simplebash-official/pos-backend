/// A login username derived from a legacy email's local part: lowercased,
/// anything outside `a-z0-9._-` becomes `-`, edges trimmed to a letter or
/// digit, padded with digits to the 3-character minimum, cut to 32, then made
/// unique against `taken` with a numeric suffix. Used only by the one-off
/// email -> username migration below.
pub(super) fn derive_username(email: &str, taken: &std::collections::HashSet<String>) -> String {
    let local = email.split('@').next().unwrap_or_default().to_lowercase();
    let cleaned: String = local
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut base = cleaned
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_string();
    base.truncate(28);
    let mut n = 1;
    while base.chars().count() < 3 {
        base.push_str(&n.to_string());
        n += 1;
    }
    let mut candidate = base.clone();
    let mut suffix = 2;
    while taken.contains(&candidate) {
        candidate = format!("{base}{suffix}");
        suffix += 1;
    }
    candidate
}
