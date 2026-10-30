use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::utils::calculate_pagination;
use crate::domain::auth::{LoginSessionListQuery, LoginSessionsResponse};
use crate::modules::auth::model::LoginSessionDocument;
use crate::modules::auth::repository;
use crate::modules::users::service as users_service;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};

/// Backs `GET /auth/sessions` — paginated (unlike `suppliers`' unpaginated
/// lists) since this is an append-only log with unbounded growth. `user_id`
/// filters to one account's `key` when the caller supplies it.
pub(crate) async fn list_sessions(
    db: &Db,
    query: LoginSessionListQuery,
) -> AppResult<LoginSessionsResponse> {
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 100);

    let filter: Document = match query.user_id {
        Some(user_id) => {
            let object_id = ObjectId::parse_str(&user_id)
                .map_err(|_| AppError::validation("Invalid userId"))?;
            let user = users_service::get_user(db, object_id).await?;
            doc! { "user_key": user.key }
        }
        None => Document::new(),
    };

    let total = repository::count_login_sessions(db, filter.clone()).await?;
    let documents = repository::list_login_sessions(db, filter, skip, limit).await?;
    let sessions = documents
        .into_iter()
        .map(LoginSessionDocument::into_login_session)
        .collect();

    Ok(LoginSessionsResponse {
        sessions,
        total,
        page,
        limit,
    })
}
