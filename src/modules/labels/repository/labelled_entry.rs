use mongodb::bson::DateTime as BsonDateTime;

/// One sticker group of a job that counts as labelled (anything not failed).
#[derive(Debug)]
pub(crate) struct LabelledEntry {
    pub job_key: String,
    pub product_key: String,
    pub copies: u32,
    pub created_at: BsonDateTime,
}
