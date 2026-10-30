// Mongo access for the `sku_counters` collection — an internal
// auto-increment sequence table, not a business entity, so unlike every
// other document in this module it does *not* get a generated `key: String`
// (see CLAUDE.md's key-prefix convention). Its `_id` is itself the
// meaningful key: the derived SKU prefix code (e.g. "PHO-SCR").

mod bump;
mod next_sequence;
mod reserve_block;
#[cfg(test)]
mod tests;

pub(crate) use next_sequence::next_sequence;
pub(crate) use reserve_block::reserve_block;
