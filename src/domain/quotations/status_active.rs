/// Lifecycle values stored in `status`. "Expired" is deliberately not one of
/// them: it is derived from `valid_until` at read time (`Quotation::is_expired`)
/// so an offer never needs a background job to flip it.
pub const STATUS_ACTIVE: &str = "active";
