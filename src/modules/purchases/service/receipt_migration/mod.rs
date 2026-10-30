// One-time MongoDB migration that gives every purchase made before supplier receipts a receipt
// to belong to (`cargo run --bin migrate_purchase_receipts`). Purchases that share a shop, a
// supplier, an invoice number and a day become one receipt; a purchase without an invoice
// number is a receipt of its own. The receipt key and number are derived from the smallest
// purchase key in the group, exactly as the SQLite migration derives them
// (`clients::sqlite::migrate_purchases_to_receipts`), so a desktop and the cloud migrating
// separately write identical rows and sync merges them. Idempotent: only purchases without a
// `receipt_key` are touched, and a receipt that already exists is left alone. The same run moves
// every purchase line from a per-unit cost to a line total (`line_total_cents` = quantity x the old
// unit cost, no share of extra charges), which is how a line is priced since extra charges and lot
// prices arrived.

mod migrate_mongo;
mod summary;

pub use migrate_mongo::migrate_mongo;
pub use summary::Summary;
