/// Who is recording the receipt, for the payment made with it.
#[derive(Debug)]
pub struct RecordedBy {
    pub user_id: String,
    pub name: String,
}
