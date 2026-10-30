// One-time MongoDB migration from per-product prices to stock batches — the
// cloud counterpart of `clients::sqlite::migrate_to_stock_batches`, which the
// desktop runs on its own SQLite file at startup. Run through
// `bin/migrate_stock_batches.rs`. For every product that still carries
// `cost_price_cents`/`selling_price_cents`:
//
// 1. creates its opening batch `stb_open_<productKey>` (same `_id`, tenant and
//    timestamps as the product) holding its current stock at its last cost and
//    selling price;
// 2. points its existing quantity-moving stock movements and its serials at
//    that batch, and gives its purchases the price their units sold at;
// 3. gives every past invoice / credit-note product line an allocation from
//    that batch (`stock_batch::backfill_line_allocations`, shared with the
//    SQLite migration so both rewrite history identically);
// 4. removes the product's price fields — last, so a product still carrying
//    them is exactly "not migrated yet" and a re-run resumes where it stopped.
//
// Every value written is derived from the product alone, so a desktop that
// migrated itself and the cloud end up with identical rows and sync merges
// them. Idempotent; safe to re-run. Works on raw collections (no tenant
// scope) because it migrates every tenant at once, copying each product's
// tenant onto its batch.
//
// `Mode::DryRun` runs every step as a read only: it counts what each step
// would change, per shop, and flags data worth a look before the real run
// (a product selling at zero, an old sale line with no cost to give it), and
// writes nothing.

mod migrate_mongo;
mod mode;
mod report;
mod single_shop;
mod summary;

pub use migrate_mongo::migrate_mongo;
pub use mode::Mode;
pub use report::Report;
pub use single_shop::SINGLE_SHOP;
pub use summary::Summary;
