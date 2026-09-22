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

/// The derived SKU prefix (e.g. "PHO-SCR") for a `category`/`subcategory`
/// name pair, without generating or consuming a number — used by the sync
/// module to discover which SKU block families a linked device needs to keep
/// pre-fetched (see `modules::sync::routes_local::sku_prefixes`).
pub(crate) fn sku_prefix(category: &str, subcategory: &str) -> String {
    format!("{}-{}", derive_code(category), derive_code(subcategory))
}

/// Generates the next SKU for a product being created under
/// `category`/`subcategory`, e.g. "PHO-SCR-0001". A linked device first draws
/// from a block the cloud pre-reserved for this exact prefix (see
/// `reserve_sku_block`/`sync::blocks`), so two offline devices creating
/// products under the same category+subcategory never mint the same SKU;
/// otherwise (unlinked, or the block is exhausted) the numeric suffix comes
/// from the atomic per-prefix counter directly — safe under concurrent
/// creates and needs no uniqueness retry loop, since both paths increment
/// the exact same counter row (see `reserve_sku_block`). Callers should
/// validate `category`/`subcategory` before calling this — a discarded
/// sequence number for a rejected request is otherwise harmless but wasteful.
pub(crate) async fn generate_sku(db: &Db, category: &str, subcategory: &str) -> AppResult<String> {
    let prefix = sku_prefix(category, subcategory);

    if let Some(pool) = db.as_sqlite()
        && let Some(taken) = crate::modules::sync::blocks::next_number(
            pool,
            &format!("sku:{prefix}"),
            &format!("{prefix}-"),
            4,
        )
        .await?
    {
        return Ok(format!("{}{:04}", taken.prefix, taken.number));
    }

    let seq = repository::sku_counter::next_sequence(db, &prefix).await?;
    Ok(format!("{prefix}-{seq:04}"))
}

/// Reserves a contiguous block of `block_size` SKU numbers under `prefix`
/// (the derived category+subcategory code, e.g. "PHO-SCR" — callers resolve
/// this the same way `generate_sku` does, via `derive_code`) for a linked
/// device to draw individual SKUs from offline. Returns the
/// `(prefix, padding, start, end)` a caller formats with
/// `format!("{prefix}{n:0padding$}")`, matching `generate_sku`'s own output
/// exactly (`prefix` here already carries the trailing `-`).
pub(crate) async fn reserve_sku_block(
    db: &Db,
    prefix: &str,
    block_size: i64,
) -> AppResult<(String, usize, i64, i64)> {
    let (start, end) = repository::sku_counter::reserve_block(db, prefix, block_size).await?;
    Ok((format!("{prefix}-"), 4, start, end))
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
