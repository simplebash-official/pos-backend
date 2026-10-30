//! Counting a supplier payment against the receipts it pays. A payment made later than the
//! receipt names the receipts and how much of itself goes to each; this checks that against what
//! each receipt still owes, and writes the settlements in the payment's own transaction. What the
//! payment does not allocate stays on the supplier's account (it still lowers the balance, which
//! is worked out from payments and purchases, not from settlements).

mod service_settlement;

pub(crate) use service_settlement::hydrate_sync_documents;
pub(crate) use service_settlement::prepare_settlements;

pub(crate) use service_settlement::write_settlements;
