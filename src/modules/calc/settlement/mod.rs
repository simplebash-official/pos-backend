//! Counting a supplier payment against the bills (receipts) it pays: what is owed together, how a
//! payment spreads over the open bills oldest first, and what is wrong with a split the person made
//! by hand. Pure; the handler reads the supplier's open bills. The save checks the same rules again
//! (`purchases::service::settlement::prepare_settlements`).

mod calc_settlement;
#[cfg(test)]
mod tests;

pub use calc_settlement::CalcSettlementPart;
pub use calc_settlement::CalcSettlementPartReply;
pub use calc_settlement::CalcSettlementReply;
pub use calc_settlement::CalcSettlementRequest;
pub use calc_settlement::OpenBill;
pub use calc_settlement::SettlementBlocker;
pub use calc_settlement::SettlementBlockerCode;
pub use calc_settlement::compute;
