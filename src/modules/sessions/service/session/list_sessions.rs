use super::populate_missing_cashier_name::populate_missing_cashier_name;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::{SessionListQuery, SessionListResponse};

use crate::modules::sessions::repository::sessions;

pub async fn list_sessions(db: &Db, query: SessionListQuery) -> AppResult<SessionListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).min(100);
    let (docs, total) = sessions::list_sessions(db, &query).await?;
    let total_pages = total.div_ceil(limit);

    let mut sessions = Vec::with_capacity(docs.len());
    for d in docs {
        let mut reg = d.into_register_session();
        populate_missing_cashier_name(db, &mut reg).await;
        sessions.push(reg);
    }

    Ok(SessionListResponse {
        sessions,
        total,
        page,
        limit,
        total_pages,
    })
}
