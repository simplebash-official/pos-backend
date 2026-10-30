use super::clean_name::clean_name;
use super::new_document::new_document;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::cash_categories::PayeeType;
use crate::modules::payees::model::PayeeDocument;
use crate::modules::payees::repository;

/// A name typed once while paying a small expense: found if already saved, else saved for next time.
pub(crate) async fn ensure_other_payee(db: &Db, raw_name: &str) -> AppResult<PayeeDocument> {
    let name = clean_name(raw_name)?;
    let document = new_document(PayeeType::Other, name, None);
    if let Some(existing) = repository::find_by_key(db, &document.key).await? {
        return Ok(existing);
    }
    repository::insert(db, document).await
}
