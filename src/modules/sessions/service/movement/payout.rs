/// One drawer payout, as the balances need it.
#[derive(Debug)]
pub(crate) struct Payout {
    pub payee_key: Option<String>,
    pub category_key: String,
    /// `YYYY-MM-DD` (UTC) the cash was handed over.
    pub day: String,
    pub number: String,
    pub amount_cents: i64,
}
