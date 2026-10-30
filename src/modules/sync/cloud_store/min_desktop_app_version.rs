/// The oldest desktop app that may sync with this cloud: 1.24.0 is the first
/// release whose purchase lines are priced by their total with a share of the
/// receipt's extra charges (and whose receipts carry those charges). An older
/// app would push purchases priced per unit and pull lines and charges it cannot
/// read, so it is held back until it updates. (1.23.0 was the first with
/// supplier receipts; 1.14.0 the first with stock batches.)
pub(crate) const MIN_DESKTOP_APP_VERSION: (u64, u64, u64) = (1, 24, 0);
