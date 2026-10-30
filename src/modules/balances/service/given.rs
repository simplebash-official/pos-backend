/// One advance handed out, from the drawer or the ledger.
#[derive(Debug)]
pub(super) struct Given {
    pub(super) employee_key: String,
    pub(super) category_key: String,
    pub(super) day: String,
    pub(super) number: String,
    pub(super) cents: i64,
}
