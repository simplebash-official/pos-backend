// Category-derived SKU generation: turns a product's category/subcategory
// into a human-readable, globally-unique SKU like "PHO-SCR-0001" so shop
// staff never have to invent product codes by hand. See `repository::sku_counter`
// for the atomic sequence that guarantees uniqueness.

use crate::{clients::db::Db, core::error::AppResult, modules::inventory::repository};

/// Reduces a name to a 3-letter uppercase code: keeps only ASCII letters,
/// takes the first 3, and pads with 'X' if the name has fewer than 3
/// letters (e.g. a two-letter or numeric-only name). Deliberately simple
/// (no vowel-dropping/stop-words) so it stays predictable — two unrelated
/// names *can* derive the same code (e.g. "Screens" and "Scanners" both ->
/// "SCR"); that's fine because `generate_sku` keys its sequence by the
/// derived code, not the raw name, so the resulting SKU is still unique.
fn derive_code(name: &str) -> String {
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

/// Generates the next SKU for a product being created under
/// `category`/`subcategory`, e.g. "PHO-SCR-0001". The numeric suffix comes
/// from an atomic per-prefix counter, so it's safe under concurrent creates
/// and needs no uniqueness retry loop. Callers should validate
/// `category`/`subcategory` before calling this — a discarded sequence
/// number for a rejected request is otherwise harmless but wasteful.
pub(crate) async fn generate_sku(db: &Db, category: &str, subcategory: &str) -> AppResult<String> {
    let prefix = format!("{}-{}", derive_code(category), derive_code(subcategory));
    let seq = repository::sku_counter::next_sequence(db, &prefix).await?;

    Ok(format!("{prefix}-{seq:04}"))
}

#[cfg(test)]
mod tests {
    use super::derive_code;

    #[test]
    fn derives_first_three_letters_uppercased() {
        assert_eq!(derive_code("Phone Repairs"), "PHO");
        assert_eq!(derive_code("Screens"), "SCR");
    }

    #[test]
    fn pads_short_names_with_x() {
        assert_eq!(derive_code("TV"), "TVX");
        assert_eq!(derive_code("A"), "AXX");
    }

    #[test]
    fn falls_back_to_all_x_for_names_without_letters() {
        assert_eq!(derive_code("24/7"), "XXX");
        assert_eq!(derive_code(""), "XXX");
    }

    #[test]
    fn strips_punctuation_and_digits_before_taking_letters() {
        // Letters after stripped punctuation/digits still start the code,
        // even if they aren't the first *word* in the name.
        assert_eq!(derive_code("24/7 Support"), "SUP");
        assert_eq!(derive_code("Mug, T-Shirt & Print Customization"), "MUG");
    }
}
