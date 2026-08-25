// Mongo access for the `users` collection only. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`
// straight from the driver and leave the "not found" -> `AppError`
// translation to `service`. Visibility is `pub(crate)` so `service` can
// call in, but the `mod repository;` declaration in `users/mod.rs` is
// private, so none of this is reachable from outside the `users` module
// tree (see `modules::suppliers::repository` for the same pattern).

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, domain::users::Role, modules::users::model::UserDocument};

fn users(db: &Database) -> Collection<UserDocument> {
    db.collection("users")
}

pub(crate) async fn find_user_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<UserDocument>> {
    Ok(users(db).find_one(doc! { "_id": id }).await?)
}

/// Looked up by normalized (lowercase) email — the entry point both
/// `create_user`'s uniqueness check and `verify_credentials`'s login lookup
/// use.
pub(crate) async fn find_user_by_email(
    db: &Database,
    email: &str,
) -> AppResult<Option<UserDocument>> {
    Ok(users(db).find_one(doc! { "email": email }).await?)
}

/// Looked up by linked employee key — backs `service::find_user_by_employee_key`,
/// the entry point `modules::employees` uses to answer "does this employee
/// already have a login" (on create-validation and on delete's
/// `EMPLOYEE_HAS_LOGIN` guard).
pub(crate) async fn find_user_by_employee_key(
    db: &Database,
    employee_key: &str,
) -> AppResult<Option<UserDocument>> {
    Ok(users(db)
        .find_one(doc! { "employee_key": employee_key })
        .await?)
}

/// Batch variant of `find_user_by_employee_key` — used by
/// `service::find_user_summaries_by_employee_keys` to enrich a whole page of
/// employees with their login summary in one query instead of one lookup
/// per row.
pub(crate) async fn find_users_by_employee_keys(
    db: &Database,
    employee_keys: &[String],
) -> AppResult<Vec<UserDocument>> {
    let mut cursor = users(db)
        .find(doc! { "employee_key": { "$in": employee_keys } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

pub(crate) async fn insert_user(
    db: &Database,
    mut document: UserDocument,
) -> AppResult<UserDocument> {
    let result = users(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// `set_doc` is built by the caller (`service::update_user`), which decides
/// which fields actually change — this just applies it.
pub(crate) async fn update_user(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<UserDocument>> {
    Ok(users(db)
        .find_one_and_update(doc! { "_id": id }, doc! { "$set": set_doc })
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_user(db: &Database, id: ObjectId) -> AppResult<Option<UserDocument>> {
    Ok(users(db).find_one_and_delete(doc! { "_id": id }).await?)
}

/// Sorted by name for stable, predictable `GET /users` output. `filter` is
/// fully assembled by the caller (`service::list_users`).
pub(crate) async fn list_users(db: &Database, filter: Document) -> AppResult<Vec<UserDocument>> {
    let mut cursor = users(db).find(filter).sort(doc! { "name": 1 }).await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Backs the single-Admin invariant in `service::create_user` — this is a
/// single-shop POS deployment, so there should never be more than one
/// Admin account.
pub(crate) async fn count_users_by_role(db: &Database, role: Role) -> AppResult<u64> {
    Ok(users(db)
        .count_documents(doc! { "role": role.as_str() })
        .await?)
}
