//! Dual-engine (Mongo / SQLite) access for the `receipt_settlements` collection: which payment
//! paid how much of which receipt. Append-only: inserted and read, never edited or deleted.

mod repository_settlement;

pub(crate) use repository_settlement::insert_settlement;
pub(crate) use repository_settlement::list_settlements_for_receipts;
