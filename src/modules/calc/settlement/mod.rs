// Counting a supplier payment against the bills (receipts) it pays: what is owed together, how a
// payment spreads over the open bills oldest first, and what is wrong with a split the person made
// by hand. Pure; the handler reads the supplier's open bills. The save checks the same rules again
// (`purchases::service::settlement::prepare_settlements`).

mod calc_settlement_part;
mod calc_settlement_part_reply;
mod calc_settlement_reply;
mod calc_settlement_request;
mod compute;
mod max_cents;
mod open_bill;
mod settlement_blocker;
mod settlement_blocker_code;
#[cfg(test)]
mod tests;

pub use calc_settlement_part::CalcSettlementPart;
pub use calc_settlement_part_reply::CalcSettlementPartReply;
pub use calc_settlement_reply::CalcSettlementReply;
pub use calc_settlement_request::CalcSettlementRequest;
pub use compute::compute;
pub use open_bill::OpenBill;
pub use settlement_blocker::SettlementBlocker;
pub use settlement_blocker_code::SettlementBlockerCode;
