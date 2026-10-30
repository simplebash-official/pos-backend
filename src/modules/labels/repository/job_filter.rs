use crate::domain::labels::LabelPrintAction;

use mongodb::bson::DateTime as BsonDateTime;

/// Filters of a job listing, already validated by `service`.
#[derive(Debug)]
pub(crate) struct JobFilter {
    pub from: Option<BsonDateTime>,
    pub to: Option<BsonDateTime>,
    pub product_key: Option<String>,
    pub action: Option<LabelPrintAction>,
}
