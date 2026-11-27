//! Internal operator endpoint to list recent business activities for a given shop.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

use super::tenants_routes::ShopActivitiesQuery;
use super::tenants_routes::ShopActivitiesResponse;
use super::tenants_routes::ShopActivityItem;
use super::tenants_routes::authorize;
use crate::app::AppState;
use crate::core::constants::codes;
use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::tenancy::{Tenant, with_tenant};
use crate::modules::billing::model::{CreditNoteDocument, InvoiceDocument};
use crate::modules::print_jobs::model::PrintJobDocument;
use crate::modules::repairs::model::RepairDocument;
use crate::modules::sessions::model::{CashMovementDocument, RegisterSessionDocument};
use crate::modules::tenants::repository;
use crate::modules::users::model::UserDocument;

/// Retrieves a unified, chronological activity feed for a shop tenant.
/// Can optionally filter by `userKey` and `kind`. Requires `x-provision-secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/activities",
    tag = modules::TENANTS,
    params(
        ("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)"),
        ShopActivitiesQuery,
    ),
    responses(
        (status = 200, description = "Paginated activity feed", body = ApiResponse<ShopActivitiesResponse>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn shop_activities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Query(query): Query<ShopActivitiesQuery>,
) -> AppResult<Json<ApiResponse<ShopActivitiesResponse>>> {
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
        .ok_or_else(|| AppError::internal("shop activities requires MongoDB"))?;
    let scope = Tenant::id(clean_id)?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(25).clamp(1, 100);
    let target_kind = query
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());

    let res = with_tenant(scope, async {
        let mut items = Vec::new();
        let mut total_count: u64 = 0;

        let user_identifiers = if let Some(ukey) = query
            .user_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            let users_col = mongo.collection::<UserDocument>("users");
            let mut ids = vec![ukey.to_string()];
            let filter = if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(ukey) {
                doc! { "$or": [{ "key": ukey }, { "employee_key": ukey }, { "_id": oid }] }
            } else {
                doc! { "$or": [{ "key": ukey }, { "employee_key": ukey }] }
            };
            if let Ok(Some(u)) = users_col.find_one(filter).await {
                ids.push(u.key);
                if let Some(oid) = u.id {
                    ids.push(oid.to_hex());
                }
                ids.push(u.username);
                if let Some(emp_key) = u.employee_key {
                    ids.push(emp_key);
                }
            }
            ids.sort();
            ids.dedup();
            Some(ids)
        } else {
            None
        };

        // 1. Invoices / Sales
        if target_kind.is_none() || target_kind == Some("sale") {
            let inv_col = mongo.collection::<InvoiceDocument>("invoices");
            let mut filter = doc! { "status": { "$ne": "voided" } };
            if let Some(ids) = &user_identifiers {
                filter.insert("cashier_id", doc! { "$in": ids });
            }
            let count = inv_col.count_documents(filter.clone()).await?;
            total_count += count;

            let mut cursor = inv_col
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .limit((page * limit) as i64)
                .await?;

            while let Some(inv) = cursor.try_next().await? {
                items.push(ShopActivityItem {
                    id: inv
                        .id
                        .map(|oid| oid.to_hex())
                        .unwrap_or_else(|| inv.key.clone()),
                    kind: "sale".to_string(),
                    reference: inv.invoice_number,
                    cashier_name: Some(inv.cashier_name_snapshot),
                    user_role: None,
                    amount_cents: Some(inv.total_cents),
                    description: format!("Completed sale via {}", inv.payment_method),
                    created_at: inv.created_at.to_chrono(),
                });
            }
        }

        // 2. Register Shifts
        if target_kind.is_none() || target_kind == Some("shift") {
            let sess_col = mongo.collection::<RegisterSessionDocument>("register_sessions");
            let mut filter = Document::new();
            if let Some(ids) = &user_identifiers {
                filter.insert("cashier_id", doc! { "$in": ids });
            }
            let count = sess_col.count_documents(filter.clone()).await?;
            total_count += count;

            let mut cursor = sess_col
                .find(filter)
                .sort(doc! { "opened_at": -1 })
                .limit((page * limit) as i64)
                .await?;

            while let Some(s) = cursor.try_next().await? {
                items.push(ShopActivityItem {
                    id: s
                        .id
                        .map(|oid| oid.to_hex())
                        .unwrap_or_else(|| s.key.clone()),
                    kind: "shift".to_string(),
                    reference: s.session_number,
                    cashier_name: Some(s.cashier_name_snapshot),
                    user_role: None,
                    amount_cents: Some(s.opening_float_cents),
                    description: format!("Register shift {:?}", s.status),
                    created_at: s.opened_at.to_chrono(),
                });
            }
        }

        // 3. Cash Movements
        if target_kind.is_none() || target_kind == Some("cash_movement") {
            let cm_col = mongo.collection::<CashMovementDocument>("cash_movements");
            let mut filter = Document::new();
            if let Some(ids) = &user_identifiers {
                filter.insert("recorded_by_user_id", doc! { "$in": ids });
            }
            let count = cm_col.count_documents(filter.clone()).await?;
            total_count += count;

            let mut cursor = cm_col
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .limit((page * limit) as i64)
                .await?;

            while let Some(cm) = cursor.try_next().await? {
                let description = match cm.note.as_deref().map(str::trim).filter(|n| !n.is_empty())
                {
                    Some(note) => format!("Cash {:?}: {}", cm.direction, note),
                    None => format!("Cash {:?}", cm.direction),
                };
                items.push(ShopActivityItem {
                    id: cm
                        .id
                        .map(|oid| oid.to_hex())
                        .unwrap_or_else(|| cm.key.clone()),
                    kind: "cash_movement".to_string(),
                    reference: cm.movement_number,
                    cashier_name: Some(cm.recorded_by_name_snapshot),
                    user_role: None,
                    amount_cents: Some(cm.amount_cents),
                    description,
                    created_at: cm.created_at.to_chrono(),
                });
            }
        }

        // 4. Returns / Credit Notes
        if target_kind.is_none() || target_kind == Some("credit_note") {
            let cn_col = mongo.collection::<CreditNoteDocument>("credit_notes");
            let mut filter = Document::new();
            if let Some(ids) = &user_identifiers {
                filter.insert("cashier_id", doc! { "$in": ids });
            }
            let count = cn_col.count_documents(filter.clone()).await?;
            total_count += count;

            let mut cursor = cn_col
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .limit((page * limit) as i64)
                .await?;

            while let Some(cn) = cursor.try_next().await? {
                items.push(ShopActivityItem {
                    id: cn
                        .id
                        .map(|oid| oid.to_hex())
                        .unwrap_or_else(|| cn.key.clone()),
                    kind: "credit_note".to_string(),
                    reference: cn.credit_note_number,
                    cashier_name: Some(cn.cashier_name_snapshot),
                    user_role: None,
                    amount_cents: Some(cn.refund_cash_cents),
                    description: "Processed return / refund credit note".to_string(),
                    created_at: cn.created_at.to_chrono(),
                });
            }
        }

        // 5. Repairs
        if target_kind.is_none() || target_kind == Some("repair") {
            let rep_col = mongo.collection::<RepairDocument>("repairs");
            let mut filter = doc! { "deleted_at": null };
            if let Some(ids) = &user_identifiers {
                filter.insert("assigned_employee_id", doc! { "$in": ids });
            }
            let count = rep_col.count_documents(filter.clone()).await?;
            total_count += count;

            let mut cursor = rep_col
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .limit((page * limit) as i64)
                .await?;

            while let Some(rep) = cursor.try_next().await? {
                items.push(ShopActivityItem {
                    id: rep
                        .id
                        .map(|oid| oid.to_hex())
                        .unwrap_or_else(|| rep.key.clone()),
                    kind: "repair".to_string(),
                    reference: rep.ticket_number,
                    cashier_name: rep.assigned_employee_name,
                    user_role: None,
                    amount_cents: Some(rep.price_cents),
                    description: format!(
                        "Repair ticket for {} (status: {})",
                        rep.device_model, rep.status
                    ),
                    created_at: rep.created_at.to_chrono(),
                });
            }
        }

        // 6. Print Jobs
        if target_kind.is_none() || target_kind == Some("print") {
            let prn_col = mongo.collection::<PrintJobDocument>("print_jobs");
            let mut filter = doc! { "deleted_at": null };
            if let Some(ids) = &user_identifiers {
                filter.insert("assigned_employee_id", doc! { "$in": ids });
            }
            let count = prn_col.count_documents(filter.clone()).await?;
            total_count += count;

            let mut cursor = prn_col
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .limit((page * limit) as i64)
                .await?;

            while let Some(prn) = cursor.try_next().await? {
                items.push(ShopActivityItem {
                    id: prn
                        .id
                        .map(|oid| oid.to_hex())
                        .unwrap_or_else(|| prn.key.clone()),
                    kind: "print".to_string(),
                    reference: prn.ticket_number,
                    cashier_name: prn.assigned_employee_name,
                    user_role: None,
                    amount_cents: Some(prn.estimated_cost_cents),
                    description: format!(
                        "Print job {} (qty: {}, status: {})",
                        prn.job_type, prn.quantity, prn.status
                    ),
                    created_at: prn.created_at.to_chrono(),
                });
            }
        }

        // Sort merged items descending by timestamp
        items.sort_by_key(|item| std::cmp::Reverse(item.created_at));

        let start = ((page - 1) * limit) as usize;
        let paginated_items = if start < items.len() {
            items.into_iter().skip(start).take(limit as usize).collect()
        } else {
            Vec::new()
        };

        Ok::<ShopActivitiesResponse, AppError>(ShopActivitiesResponse {
            items: paginated_items,
            total: total_count,
            page,
            limit,
        })
    })
    .await?;

    Ok(Json(ApiResponse::success(res, "Shop activities")))
}
