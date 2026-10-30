use crate::domain::cash_categories::{PayeeType, normalize_name, payee_key};
use crate::modules::payees::model::PayeeDocument;
use mongodb::bson::DateTime as BsonDateTime;

pub(super) fn new_document(
    payee_type: PayeeType,
    name: String,
    phone: Option<String>,
) -> PayeeDocument {
    let now = BsonDateTime::now();
    PayeeDocument {
        id: None,
        key: payee_key(payee_type, &name),
        payee_type,
        normalized_name: normalize_name(&name),
        name,
        phone,
        active: true,
        version: 1,
        created_at: now,
        updated_at: now,
        updated_by_device: None,
    }
}
