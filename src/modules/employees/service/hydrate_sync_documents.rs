use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::employees::Employee;
use crate::modules::employees::model::EmployeeDocument;
use crate::modules::users;
use mongodb::bson::Document;

/// Converts a page of raw `employees` documents — as read by the sync
/// module's cursor scan — into the `Employee` shape the REST reads return.
/// Unlike every other module's `hydrate_sync_documents`, this one is `async`
/// and takes `db`: the `login` field needs the same cross-module lookup a
/// REST read performs, which a pure deserialize-and-convert can't do. See
/// `suppliers::service::hydrate_sync_documents` for why the delta and
/// snapshot feeds must otherwise produce identical rows.
pub(crate) async fn hydrate_sync_documents(
    db: &Db,
    documents: Vec<Document>,
) -> AppResult<Vec<Employee>> {
    let docs: Vec<EmployeeDocument> = documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<EmployeeDocument>(
                document,
            )?)
        })
        .collect::<AppResult<Vec<_>>>()?;

    let keys: Vec<String> = docs.iter().map(|d| d.key.clone()).collect();
    let mut logins = users::service::find_user_summaries_by_employee_keys(db, &keys).await?;

    Ok(docs
        .into_iter()
        .map(|document| {
            let login = logins.remove(&document.key);
            document.into_employee(login)
        })
        .collect())
}
