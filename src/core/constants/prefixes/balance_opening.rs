/// Account opening balances use fixed keys `aopen_<kind>` so the same account set up on two devices is one row.
/// Balance openings use fixed keys `bopen_<subject key>`.
pub const BALANCE_OPENING: &str = "bopen";
