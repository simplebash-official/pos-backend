use super::find_admin_document::find_admin_document;

use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::users::User;

use crate::modules::users::repository;

use mongodb::bson::doc;

/// Resolves the local user behind an identity-server account token. A shop's
/// owner is its single Admin (the `admin` login provisioned at shop creation),
/// so the owner scope maps to that account. Identity owns the display name, so
/// a changed `name` claim is written through. `None` when the shop has no Admin.
pub(crate) async fn find_user_for_identity(db: &Db, name: Option<&str>) -> AppResult<Option<User>> {
    let Some(mut document) = find_admin_document(db).await? else {
        return Ok(None);
    };
    if let (Some(name), Some(id)) = (name.map(str::trim).filter(|n| !n.is_empty()), document.id)
        && document.name != name
        && let Some(updated) = repository::update_user(db, id, doc! { "name": name }).await?
    {
        document = updated;
    }
    Ok(Some(document.into_user()))
}
