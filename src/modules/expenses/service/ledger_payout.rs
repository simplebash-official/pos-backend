/// One ledger payment, as the balances need it.
#[derive(Debug)]
pub(crate) struct LedgerPayout {
    pub payee_key: Option<String>,
    pub category_key: String,
    /// `YYYY-MM-DD` the money left.
    pub day: String,
    pub number: String,
    pub amount_cents: i64,
}
