//! Category-derived SKU generation: turns a product's category/subcategory
//! into a human-readable, globally-unique SKU like "PHO-SCR-0001" so shop
//! staff never have to invent product codes by hand. See `repository::sku_counter`
//! for the atomic sequence that guarantees uniqueness.

mod service_sku;
#[cfg(test)]
mod tests;

pub(crate) use service_sku::generate_sku;
pub(crate) use service_sku::reserve_sku_block;
pub(crate) use service_sku::sku_prefix;
