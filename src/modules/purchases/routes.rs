// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there. No placeholder status route here for
// the same reason as `suppliers`/`supplier_products`: the collection root
// is a real endpoint. Every route — reads included — requires `AdminUser`,
// matching `suppliers`/`supplier_products`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::AdminUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::purchases::{CreatePurchaseRequest, Purchase, PurchaseListQuery, PurchaseListResponse},
    modules::purchases::service::purchase,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_purchases, record_purchase))
}

// ============================================================================
// Purchases
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::PURCHASES, params(PurchaseListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Purchase history for a supplier and/or product, or full collection paginated", body = ApiResponse<PurchaseListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
async fn list_purchases(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<PurchaseListQuery>,
) -> AppResult<Json<ApiResponse<PurchaseListResponse>>> {
    let response = purchase::list_purchases(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Purchases retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/", tag = modules::PURCHASES, request_body = CreatePurchaseRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Stock received and recorded", body = ApiResponse<Purchase>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or product not found", body = ErrorResponse),
    )
)]
async fn record_purchase(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreatePurchaseRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Purchase>>)> {
    let recorded = purchase::record_purchase(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            recorded,
            "Stock received successfully",
        )),
    ))
}
