use super::manageable_roles::manageable_roles;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::users::{Role, UserListQuery, UsersResponse};

use crate::modules::users::model::UserDocument;
use crate::modules::users::repository;

use mongodb::bson::{Document, doc};

/// Builds the Mongo filter from query params (free-text search across
/// name/username, plus an exact-role filter) — same construction style as
/// `suppliers::service::list_suppliers`. Not paginated: a shop's staff
/// roster is small enough to return in full. Always scoped to
/// `manageable_roles(caller_role)` regardless of what `query.role` asks
/// for — a Manager's list is always Staff-only, and an Admin's list never
/// includes the (one) Admin account, matching `get`/`update`/`delete`'s
/// same restriction. Asking for a role outside that scope (e.g. a Manager
/// filtering `role=admin`) returns an empty list rather than an error, the
/// same "just don't show it" spirit as `ensure_manageable`'s 404.
pub(crate) async fn list_users(
    db: &Db,
    query: UserListQuery,
    caller_role: Role,
) -> AppResult<UsersResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = crate::core::utils::build_bson_regex(&search);
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "username": { "$regex": pattern } },
            ]
        });
    }

    let allowed = manageable_roles(caller_role);
    match query.role {
        Some(role) if allowed.contains(&role) => {
            and_clauses.push(doc! { "role": role.as_str() });
        }
        Some(_) => and_clauses.push(doc! { "role": { "$in": Vec::<String>::new() } }),
        None => {
            let allowed_strs: Vec<&str> = allowed.iter().map(Role::as_str).collect();
            and_clauses.push(doc! { "role": { "$in": allowed_strs } });
        }
    }

    let filter = doc! { "$and": and_clauses };

    let documents = repository::list_users(db, filter).await?;
    let users = documents.into_iter().map(UserDocument::into_user).collect();

    Ok(UsersResponse { users })
}
