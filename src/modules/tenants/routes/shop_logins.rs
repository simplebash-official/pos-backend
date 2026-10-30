// Internal operator endpoint to list login history for a given shop.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

use super::authorize::authorize;
use super::shop_login_item::ShopLoginItem;
use super::shop_logins_query::ShopLoginsQuery;
use super::shop_logins_response::ShopLoginsResponse;
use crate::app::AppState;
use crate::core::constants::codes;
use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::tenancy::{Tenant, with_tenant};
use crate::modules::auth::model::LoginSessionDocument;
use crate::modules::tenants::repository;

/// Retrieves a paginated login audit trail for a shop tenant.
/// Can optionally filter by `userKey`. Requires `x-provision-secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/logins",
    tag = modules::TENANTS,
    params(
        ("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)"),
        ShopLoginsQuery,
    ),
    responses(
        (status = 200, description = "Paginated login history", body = ApiResponse<ShopLoginsResponse>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn shop_logins(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Query(query): Query<ShopLoginsQuery>,
) -> AppResult<Json<ApiResponse<ShopLoginsResponse>>> {
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
        .ok_or_else(|| AppError::internal("shop logins requires MongoDB"))?;
    let scope = Tenant::id(clean_id)?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(25).clamp(1, 100);

    let res = with_tenant(scope, async {
        let sessions_col = mongo.collection::<LoginSessionDocument>("login_sessions");
        let mut filter = Document::new();
        if let Some(user_key) = query
            .user_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            filter.insert("user_key", user_key);
        }

        let total = sessions_col.count_documents(filter.clone()).await?;
        let mut cursor = sessions_col
            .find(filter)
            .sort(doc! { "created_at": -1 })
            .skip((page - 1) * limit)
            .limit(limit as i64)
            .await?;

        let mut items = Vec::new();
        while let Some(d) = cursor.try_next().await? {
            items.push(ShopLoginItem {
                id: d
                    .id
                    .map(|oid| oid.to_hex())
                    .unwrap_or_else(|| d.key.clone()),
                key: d.key,
                user_key: d.user_key,
                name: d.name_at_login,
                username: d.username_at_login,
                role: d.role_at_login,
                ip_address: d.ip_address,
                user_agent: d.user_agent,
                logged_in_at: d.created_at.to_chrono(),
            });
        }

        Ok::<ShopLoginsResponse, AppError>(ShopLoginsResponse {
            items,
            total,
            page,
            limit,
        })
    })
    .await?;

    Ok(Json(ApiResponse::success(res, "Shop logins")))
}
