//! Number blocks for linked devices (SQLite). Document numbers (invoice, credit
//! note, ticket...) must not collide across offline devices, so a linked device
//! draws them from ranges the cloud reserved for it (`sync_number_blocks`,
//! filled by the shell's sync agent through `POST /api/sync/blocks`). When a
//! block runs dry offline, numbers fall back to `<prefix>D<deviceTag>-<n>`,
//! which is unique because cloud numbers never contain the `D<tag>-` infix.
//! An unlinked device keeps the plain local counter (see `sequences::service`).

mod sync_blocks;

pub(crate) use sync_blocks::device_numeric_tag;
pub(crate) use sync_blocks::local_sku_prefixes;
pub(crate) use sync_blocks::next_number;
pub(crate) use sync_blocks::next_raw_number;
pub use sync_blocks::store_block;
