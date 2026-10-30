// Number blocks for linked devices (SQLite). Document numbers (invoice, credit
// note, ticket...) must not collide across offline devices, so a linked device
// draws them from ranges the cloud reserved for it (`sync_number_blocks`,
// filled by the shell's sync agent through `POST /api/sync/blocks`). When a
// block runs dry offline, numbers fall back to `<prefix>D<deviceTag>-<n>`,
// which is unique because cloud numbers never contain the `D<tag>-` infix.
// An unlinked device keeps the plain local counter (see `sequences::service`).

mod canonical_name;
mod device_number;
mod device_numeric_tag;
mod local_sku_prefixes;
mod next_number;
mod next_raw_number;
mod store_block;

pub(crate) use device_numeric_tag::device_numeric_tag;
pub(crate) use local_sku_prefixes::local_sku_prefixes;
pub(crate) use next_number::next_number;
pub(crate) use next_raw_number::next_raw_number;
pub use store_block::store_block;
