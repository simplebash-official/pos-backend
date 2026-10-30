// The categories every shop starts with (the "starting point" of the cash movement
// design). They are ordinary rows the shop can rename, hide or reorder; only the
// deterministic key (`domain::cash_categories::category_key`) marks them as the same
// row on every device. Idempotent: an existing row is never overwritten, so a shop's
// edits survive a re-run.

mod cash_category_seed_result;
mod seed_cash_categories;
mod system_categories;

pub use cash_category_seed_result::CashCategorySeedResult;
pub use seed_cash_categories::seed_cash_categories;
pub use system_categories::SYSTEM_CATEGORIES;
