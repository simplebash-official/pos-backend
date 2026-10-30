use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::users::Role;

use crate::modules::users::model::UserDocument;
use crate::modules::users::repository;

use mongodb::bson::doc;

/// The shop's single Admin document, if one exists.
pub(super) async fn find_admin_document(db: &Db) -> AppResult<Option<UserDocument>> {
    Ok(
        repository::list_users(db, doc! { "role": Role::Admin.as_str() })
            .await?
            .into_iter()
            .next(),
    )
}
