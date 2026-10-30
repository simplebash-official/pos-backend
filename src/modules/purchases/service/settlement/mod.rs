// Counting a supplier payment against the receipts it pays. A payment made later than the
// receipt names the receipts and how much of itself goes to each; this checks that against what
// each receipt still owes, and writes the settlements in the payment's own transaction. What the
// payment does not allocate stays on the supplier's account (it still lowers the balance, which
// is worked out from payments and purchases, not from settlements).

mod hydrate_sync_documents;
mod prepare_settlements;
mod prepared_settlements;
mod write_settlements;

pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub(crate) use prepare_settlements::prepare_settlements;

pub(crate) use write_settlements::write_settlements;
