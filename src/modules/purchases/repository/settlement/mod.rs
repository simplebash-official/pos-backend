// Dual-engine (Mongo / SQLite) access for the `receipt_settlements` collection: which payment
// paid how much of which receipt. Append-only: inserted and read, never edited or deleted.

mod insert_settlement;
mod list_settlements_for_receipts;
mod settlements;

pub(crate) use insert_settlement::insert_settlement;
pub(crate) use list_settlements_for_receipts::list_settlements_for_receipts;
