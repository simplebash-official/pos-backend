// Internal operator endpoint to list all POS user accounts for a given shop.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

use super::authorize::authorize;
use super::shop_user_response::ShopUserResponse;
use crate::app::AppState;
use crate::core::constants::codes;
use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::tenancy::{Tenant, with_tenant};
use crate::modules::tenants::repository;
use crate::modules::users::model::UserDocument;

/// Lists all POS user accounts belonging to a shop tenant, enriched with each user's
/// most recent login timestamp. Requires `x-provision-secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/users",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    responses(
        (status = 200, description = "List of shop POS users", body = ApiResponse<Vec<ShopUserResponse>>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn shop_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> AppResult<Json<ApiResponse<Vec<ShopUserResponse>>>> {
    authorize(&state, &headers)?;
    let clean_id = tenant_id.trim();
    let _tenant = repository::find_tenant_by_key(&state.db, clean_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code(
                format!("No shop with id '{clean_id}'"),
                codes::TENANT_NOT_FOUND,
            )
        })?;
    let mongo = state
        .db
        .as_mongo()
        .ok_or_else(|| AppError::internal("shop users requires MongoDB"))?;
    let scope = Tenant::id(clean_id)?;

    let users = with_tenant(scope, async {
        let users_col = mongo.collection::<UserDocument>("users");
        let mut user_cursor = users_col
            .find(doc! {})
            .sort(doc! { "created_at": 1 })
            .await?;

        let mut user_docs = Vec::new();
        while let Some(u) = user_cursor.try_next().await? {
            user_docs.push(u);
        }

        let sessions_col = mongo.collection::<Document>("login_sessions");
        let mut session_cursor = sessions_col
            .aggregate(vec![
                doc! { "$group": { "_id": "$user_key", "last": { "$max": "$created_at" } } },
            ])
            .await?;

        let mut last_logins: HashMap<String, DateTime<Utc>> = HashMap::new();
        while let Some(row) = session_cursor.try_next().await? {
            if let Ok(ukey) = row.get_str("_id")
                && let Ok(last_dt) = row.get_datetime("last")
            {
                last_logins.insert(ukey.to_string(), last_dt.to_chrono());
            }
        }

        let result = user_docs
            .into_iter()
            .map(|u| {
                let last_login = last_logins.get(&u.key).copied();
                ShopUserResponse {
                    id: u
                        .id
                        .map(|oid| oid.to_hex())
                        .unwrap_or_else(|| u.key.clone()),
                    key: u.key,
                    username: u.username,
                    name: u.name,
                    role: u.role,
                    is_active: u.is_active,
                    employee_key: u.employee_key,
                    created_at: u.created_at.to_chrono(),
                    updated_at: u.updated_at.to_chrono(),
                    last_login_at: last_login,
                }
            })
            .collect::<Vec<_>>();
        Ok::<Vec<ShopUserResponse>, AppError>(result)
    })
    .await?;

    Ok(Json(ApiResponse::success(users, "Shop users")))
}
