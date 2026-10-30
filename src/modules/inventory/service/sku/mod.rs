// Category-derived SKU generation: turns a product's category/subcategory
// into a human-readable, globally-unique SKU like "PHO-SCR-0001" so shop
// staff never have to invent product codes by hand. See `repository::sku_counter`
// for the atomic sequence that guarantees uniqueness.

mod derive_code;
mod generate_sku;
mod reserve_sku_block;
mod sku_prefix;
#[cfg(test)]
mod tests;

pub(crate) use generate_sku::generate_sku;
pub(crate) use reserve_sku_block::reserve_sku_block;
pub(crate) use sku_prefix::sku_prefix;
