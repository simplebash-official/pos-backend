// ---------------------------------------------------------------------------
// Drawer rules: the one copy, used by the cash movement save, the shift open/close save and the
// live calculations (`modules::calc::cash`). Integer cents, no I/O.
// ---------------------------------------------------------------------------

/// Small variance (Rs 5.00) a cashier can close a shift with on a note alone.
pub const VARIANCE_TOLERANCE_CENTS: i64 = 500;
