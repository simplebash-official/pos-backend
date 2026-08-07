// Business logic for the `barcode` module: EAN-13 encoding (pure, no DB)
// and the namespaced counter-backed generator built on top of it. Calls
// into `super::repository` for the one piece of state that's genuinely
// dangerous to get wrong (the atomic sequence) and returns finished barcode
// strings to callers — never BSON/Mongo types.

pub(crate) mod ean13;
pub(crate) mod generator;
