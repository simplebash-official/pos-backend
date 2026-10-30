// Mongo access for the `barcode_counters` collection only — one document
// per namespace (e.g. `"product"`). Visibility is `pub(crate)` so `service`
// can call in, but the `mod repository;` declaration in `barcode/mod.rs` is
// private, so none of this is reachable from outside this module tree.

mod barcode_counters;
mod next_sequence;
mod reserve_block;
#[cfg(test)]
mod tests;

pub(crate) use next_sequence::next_sequence;
pub(crate) use reserve_block::reserve_block;
