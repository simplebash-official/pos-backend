/// Reduces a name to a 3-letter uppercase code: keeps only ASCII letters,
/// takes the first 3, and pads with 'X' if the name has fewer than 3
/// letters (e.g. a two-letter or numeric-only name). Deliberately simple
/// (no vowel-dropping/stop-words) so it stays predictable — two unrelated
/// names *can* derive the same code (e.g. "Screens" and "Scanners" both ->
/// "SCR"); that's fine because `generate_sku` keys its sequence by the
/// derived code, not the raw name, so the resulting SKU is still unique.
pub(super) fn derive_code(name: &str) -> String {
    let letters: String = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();

    let mut code: String = letters.chars().take(3).collect();
    while code.len() < 3 {
        code.push('X');
    }
    code
}
