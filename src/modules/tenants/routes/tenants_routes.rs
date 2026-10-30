use super::shop_activities;
use crate::app::AppState;
use crate::core::config::Provisioning;
use crate::core::constants::{codes, modules};
use crate::core::error::{AppError, AppResult};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::tenancy::{Tenant, with_tenant};
use crate::domain::users::Role;
use crate::modules::auth::model::LoginSessionDocument;
use crate::modules::tenants::service::ProvisionOwner;
use crate::modules::tenants::usage::TenantUsage;
use crate::modules::tenants::{repository, service, usage};
use crate::modules::users::model::UserDocument;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

const PROVISION_SECRET_HEADER: &str = "x-provision-secret";

/// The endpoint only exists in a multi-tenant deployment that configured a
/// secret; anywhere else it answers 404 as if it were not routed at all.
pub(super) fn authorize(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    let Provisioning::Enabled { secret: expected } = &state.config.provisioning else {
        return Err(AppError::not_found("Not found"));
    };
    let provided = headers
        .get(PROVISION_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if secrets_match(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(AppError::unauthorized("Invalid provisioning secret"))
    }
}

/// Usage of several shops in one call (the console's lists). Requires `X-Provision-Secret`.
#[utoipa::path(
    post,
    path = "/tenants/usage",
    tag = modules::TENANTS,
    request_body = BatchUsageRequest,
    responses(
        (status = 200, description = "Usage figures", body = ApiResponse<BatchUsageResponse>),
        (status = 400, description = "More than 100 shops", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "The endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn batch_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BatchUsageRequest>,
) -> AppResult<Json<ApiResponse<BatchUsageResponse>>> {
    authorize(&state, &headers)?;
    let ids: Vec<String> = body
        .tenant_ids
        .iter()
        .map(|i| i.trim().to_string())
        .collect();
    let items = usage::usage_for_many(&state.db, &ids).await?;
    Ok(Json(ApiResponse::success(
        BatchUsageResponse { items },
        "Shop usage",
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUsageRequest {
    /// Up to 100 tenant ids; ids that are not in the directory are left out of the answer.
    pub tenant_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUsageResponse {
    pub items: Vec<TenantUsage>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
/// The design a shop selected for one document type.
pub struct DocumentSelection {
    /// The document kind (`a4-invoice`, `thermal-receipt`, `credit-note`, `quotation`).
    pub kind: String,
    /// The design family the shop chose for it.
    pub family: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSelectionsResponse {
    pub selections: Vec<DocumentSelection>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionOwnerRequest {
    /// Always `admin`; the shop code + this username is the POS login.
    pub username: String,
    pub name: String,
    /// Argon2id PHC string of the admin password the owner chose for this shop (`$argon2id$...`), never a plaintext password.
    pub password_hash: String,
}

/// Registers a shop created by the identity service and, optionally, its first
/// Admin. Idempotent; requires the shared `X-Provision-Secret` header.
#[utoipa::path(
    post,
    path = "/provision",
    tag = modules::TENANTS,
    request_body = ProvisionShopRequest,
    responses(
        (status = 200, description = "Shop registered (or already registered)", body = ApiResponse<ProvisionShopResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Provisioning is not enabled on this deployment", body = ErrorResponse),
        (status = 409, description = "Shop code or tenant id already used by a different shop", body = ErrorResponse),
    )
)]
pub(super) async fn provision_shop(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProvisionShopRequest>,
) -> AppResult<Json<ApiResponse<ProvisionShopResponse>>> {
    authorize(&state, &headers)?;

    let owner = body.owner.map(|o| ProvisionOwner {
        username: o.username,
        name: o.name,
        password_hash: o.password_hash,
    });
    let outcome = service::provision_shop(
        &state.db,
        body.tenant_id.trim(),
        &body.shop_code,
        &body.name,
        owner,
    )
    .await?;

    Ok(Json(ApiResponse::success(
        ProvisionShopResponse {
            tenant_created: outcome.tenant_created,
            admin_created: outcome.admin_created,
        },
        "Shop provisioned",
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionShopRequest {
    /// The identity service's tenant id (`tnt_...`).
    pub tenant_id: String,
    pub shop_code: String,
    pub name: String,
    /// Present for web sign-ups; absent for desktop ones (their admin arrives via device sync).
    pub owner: Option<ProvisionOwnerRequest>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionShopResponse {
    /// False when the shop was already registered (a replay).
    pub tenant_created: bool,
    /// False when the shop already had an Admin, or no owner was supplied.
    pub admin_created: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetSelectionsRequest {
    /// Design families whose choice (and saved option values) is cleared.
    pub families: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetSelectionsResponse {
    /// The `(kind, family)` choices that were cleared.
    pub reset: Vec<DocumentSelection>,
}

/// Clears a shop's choice of the given design families (and the option values
/// it saved for them): an explicit operator action taken together with
/// revoking the design. Idempotent. Requires `X-Provision-Secret`.
#[utoipa::path(
    post,
    path = "/tenants/{tenant_id}/document-selections/reset",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    request_body = ResetSelectionsRequest,
    responses(
        (status = 200, description = "Choices cleared", body = ApiResponse<ResetSelectionsResponse>),
        (status = 400, description = "No families given", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn reset_shop_document_selections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<ResetSelectionsRequest>,
) -> AppResult<Json<ApiResponse<ResetSelectionsResponse>>> {
    authorize(&state, &headers)?;
    if body.families.is_empty() {
        return Err(AppError::validation(
            "families must name at least one design",
        ));
    }
    let reset =
        service::reset_document_selections(&state.db, tenant_id.trim(), &body.families).await?;
    Ok(Json(ApiResponse::success(
        ResetSelectionsResponse {
            reset: to_selections(reset),
        },
        "Document selections reset",
    )))
}

/// Routes of the tenants module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(provision_shop))
        .routes(routes!(set_status))
        .routes(routes!(shop_usage))
        .routes(routes!(batch_usage))
        .routes(routes!(shop_document_selections))
        .routes(routes!(reset_shop_document_selections))
        .routes(routes!(shop_users))
        .routes(routes!(shop_logins))
        .routes(routes!(shop_activities::shop_activities))
}

/// Compares without an early exit on the first differing byte, so response time
/// doesn't reveal how much of a guessed secret was right.
pub(super) fn secrets_match(provided: &[u8], expected: &[u8]) -> bool {
    let mut diff = provided.len() ^ expected.len();
    for i in 0..provided.len().max(expected.len()) {
        let a = provided.get(i).copied().unwrap_or(0);
        let b = expected.get(i).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

/// Suspends or reinstates a shop. While suspended, every request carrying a token of the shop is
/// refused with 403 `ACCOUNT_SUSPENDED` carrying `details` `{suspendedAt, reason, retained}`
/// (within about 30 s on every instance) and shop-code
/// logins are refused after the password check. Idempotent; requires `X-Provision-Secret`.
#[utoipa::path(
    put,
    path = "/tenants/{tenant_id}/status",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    request_body = SetStatusRequest,
    responses(
        (status = 200, description = "Status applied", body = ApiResponse<StatusResponse>),
        (status = 400, description = "Malformed tenant id", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn set_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<SetStatusRequest>,
) -> AppResult<Json<ApiResponse<StatusResponse>>> {
    authorize(&state, &headers)?;
    let tenant_id = tenant_id.trim().to_string();
    service::set_suspended(&state.db, &tenant_id, body.suspension()?).await?;
    Ok(Json(ApiResponse::success(
        StatusResponse {
            tenant_id,
            suspended: body.suspended,
        },
        if body.suspended {
            "Shop suspended"
        } else {
            "Shop reinstated"
        },
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusRequest {
    /// True switches the shop's cloud access off; false switches it back on.
    pub suspended: bool,
    /// When the account was suspended (RFC 3339). Required with `suspended: true`, absent or null
    /// with `suspended: false`.
    pub suspended_at: Option<String>,
    /// Why (1-500 characters). Same presence rule as `suspended_at`; shown to the shop's staff.
    pub reason: Option<String>,
}

impl SetStatusRequest {
    /// The suspension to store, or `None` to reinstate; 400 when the fields do not match `suspended`.
    pub(super) fn suspension(&self) -> AppResult<Option<(mongodb::bson::DateTime, &str)>> {
        match (self.suspended, &self.suspended_at, &self.reason) {
            (false, None, None) => Ok(None),
            (false, _, _) => Err(AppError::validation(
                "suspendedAt and reason must be absent when suspended is false",
            )),
            (true, Some(at), Some(reason)) => {
                let reason = reason.trim();
                if reason.is_empty() || reason.chars().count() > 500 {
                    return Err(AppError::validation("reason must be 1-500 characters"));
                }
                let at = chrono::DateTime::parse_from_rfc3339(at)
                    .map_err(|_| AppError::validation("suspendedAt must be an RFC 3339 time"))?;
                Ok(Some((
                    mongodb::bson::DateTime::from_chrono(at.to_utc()),
                    reason,
                )))
            }
            (true, _, _) => Err(AppError::validation(
                "suspendedAt and reason are required when suspended is true",
            )),
        }
    }
}

// Query parameters for filtering and paginating a shop's business activities.
/// Query parameters for GET /tenants/{tenant_id}/activities.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ShopActivitiesQuery {
    pub user_key: Option<String>,
    pub kind: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

// Paginated response of business activities for a shop.
/// Paginated business activities result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopActivitiesResponse {
    pub items: Vec<ShopActivityItem>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}

// Representation of a normalized business activity record for operator audit.
/// Details of a single business activity item.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopActivityItem {
    pub id: String,
    /// Type of activity: "sale", "shift", "cash_movement", "credit_note", "repair", "print".
    pub kind: String,
    /// Reference number (e.g. invoice number, session number, movement number).
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cashier_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_cents: Option<i64>,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

/// The document designs a shop has chosen, so the console can tell whether a
/// design is in use before revoking it. Requires `X-Provision-Secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/document-selections",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    responses(
        (status = 200, description = "The shop's chosen designs", body = ApiResponse<DocumentSelectionsResponse>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn shop_document_selections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> AppResult<Json<ApiResponse<DocumentSelectionsResponse>>> {
    authorize(&state, &headers)?;
    let selections = service::document_selections(&state.db, tenant_id.trim()).await?;
    Ok(Json(ApiResponse::success(
        DocumentSelectionsResponse {
            selections: to_selections(selections),
        },
        "Document selections",
    )))
}

// Representation of a single login session record for operator audit.
/// Details of a single user login session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopLoginItem {
    pub id: String,
    pub key: String,
    pub user_key: String,
    pub name: String,
    pub username: String,
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    pub logged_in_at: DateTime<Utc>,
}

// Internal operator endpoint to list login history for a given shop.
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

// Query parameters for filtering and paginating a shop's login history.
/// Query parameters for GET /tenants/{tenant_id}/logins.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ShopLoginsQuery {
    pub user_key: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

// Paginated response of login sessions for a shop.
/// Paginated login sessions result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopLoginsResponse {
    pub items: Vec<ShopLoginItem>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}

/// How much one shop uses the POS: bill counts, gross sales, catalogue and staff sizes, devices
/// and the last sync. Counts and totals only, no record content. Requires `X-Provision-Secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/usage",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    responses(
        (status = 200, description = "Usage figures", body = ApiResponse<TenantUsage>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn shop_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> AppResult<Json<ApiResponse<TenantUsage>>> {
    authorize(&state, &headers)?;
    let usage = usage::usage_for(&state.db, tenant_id.trim()).await?;
    Ok(Json(ApiResponse::success(usage, "Shop usage")))
}

// Response shape for a shop's POS user account in the operator console.
/// A single POS staff account belonging to a shop tenant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopUserResponse {
    pub id: String,
    pub key: String,
    pub username: String,
    pub name: String,
    pub role: Role,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<DateTime<Utc>>,
}

// Internal operator endpoint to list all POS user accounts for a given shop.
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

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub tenant_id: String,
    pub suspended: bool,
}

pub(super) fn to_selections(pairs: Vec<(String, String)>) -> Vec<DocumentSelection> {
    pairs
        .into_iter()
        .map(|(kind, family)| DocumentSelection { kind, family })
        .collect()
}
