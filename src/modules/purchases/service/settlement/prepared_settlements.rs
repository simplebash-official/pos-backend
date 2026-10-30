/// Settlements checked and ready to write once the payment's key is known.
#[derive(Debug)]
pub(crate) struct PreparedSettlements {
    pub(super) supplier_key: String,
    pub(super) parts: Vec<(String, i64)>,
}
