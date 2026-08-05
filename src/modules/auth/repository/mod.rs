// Mongo access for the `login_sessions` collection only — auth's own
// append-only audit log. `auth` never touches the `users` collection
// directly (see `modules::auth::service::login`, which reaches account data
// exclusively through `users::service`). Visibility is `pub(crate)` so
// `service` can call in, but the `mod repository;` declaration in
// `auth/mod.rs` is private, so none of this is reachable from outside the
// `auth` module tree.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc},
};

use crate::{core::error::AppResult, modules::auth::model::LoginSessionDocument};

fn login_sessions(db: &Database) -> Collection<LoginSessionDocument> {
    db.collection("login_sessions")
}

pub(crate) async fn insert_login_session(
    db: &Database,
    mut document: LoginSessionDocument,
) -> AppResult<LoginSessionDocument> {
    let result = login_sessions(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// Sorted `created_at` descending (most recent login first) — `filter` is
/// fully assembled by the caller (`service::list_sessions`).
pub(crate) async fn list_login_sessions(
    db: &Database,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<Vec<LoginSessionDocument>> {
    let mut cursor = login_sessions(db)
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

pub(crate) async fn count_login_sessions(db: &Database, filter: Document) -> AppResult<u64> {
    Ok(login_sessions(db).count_documents(filter).await?)
}
