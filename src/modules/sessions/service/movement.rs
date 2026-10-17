// Cash movement service: pay-in, pay-out, owner withdrawal, cash drop, and drawer open events.

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::{
        sequences::ReserveSequenceRequest,
        sessions::{
            CashMovement, CashMovementListQuery, CashMovementListResponse, CashMovementType,
            CreateCashMovementRequest,
        },
    },
    modules::{
        sequences,
        sessions::{
            model::CashMovementDocument,
            repository::{drawer, movements, sessions},
        },
        users,
    },
};

/// Configurable threshold above which owner withdrawal requires an Admin (Owner) PIN specifically.
pub const OWNER_WITHDRAWAL_ADMIN_THRESHOLD_CENTS: i64 = 50_000;

pub async fn record_movement(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateCashMovementRequest,
    device_id: Option<String>,
) -> AppResult<CashMovement> {
    if body.terminal_id.trim().is_empty() {
        return Err(AppError::validation("terminalId is required"));
    }
    if body.reason.trim().is_empty() {
        return Err(AppError::validation("Reason is required"));
    }

    if body.movement_type != CashMovementType::DrawerOpenNoSale && body.amount_cents <= 0 {
        return Err(AppError::validation("Movement amount must be greater than zero"));
    }

    let mut approved_by_id = None;
    let mut approved_by_name = None;

    // Owner withdrawals require PIN authorization (and if above threshold, owner/admin specifically)
    if body.movement_type == CashMovementType::OwnerWithdrawal {
        if body.recipient_or_source.as_deref().unwrap_or("").trim().is_empty() {
            return Err(AppError::validation("Recipient is required for owner withdrawal"));
        }

        let pin = body.manager_pin.as_deref().ok_or_else(|| {
            AppError::forbidden_with_code(
                "PIN authorization is required for owner withdrawal",
                codes::PIN_NOT_SET,
            )
        })?;

        let required_role = if body.amount_cents > OWNER_WITHDRAWAL_ADMIN_THRESHOLD_CENTS {
            "admin"
        } else {
            "manager"
        };

        let approver = users::service::verify_user_pin(db, pin, Some(required_role))
            .await?
            .ok_or_else(|| {
                if required_role == "admin" {
                    AppError::forbidden_with_code(
                        "Owner/Admin PIN is required for withdrawals exceeding the owner threshold",
                        codes::OWNER_APPROVAL_REQUIRED,
                    )
                } else {
                    AppError::unauthorized_with_code("Invalid manager/owner PIN", codes::INVALID_PIN)
                }
            })?;

        approved_by_id = Some(approver.id);
        approved_by_name = Some(approver.name);
    }

    let reservation = sequences::service::reserve_sequence(
        db,
        "cash_movement".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id,
        },
    )
    .await?;

    let movement_number = format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    );

    let now = BsonDateTime::now();
    let movement_doc = CashMovementDocument {
        id: None,
        key: generate_id(prefixes::CASH_MOVEMENT),
        movement_number,
        movement_type: body.movement_type,
        amount_cents: body.amount_cents,
        session_key: body.session_key.clone(),
        terminal_id: body.terminal_id.clone(),
        reason: body.reason,
        recipient_or_source: body.recipient_or_source,
        supplier_key: body.supplier_key,
        recorded_by_user_id,
        recorded_by_name_snapshot: recorded_by_name,
        approved_by_user_id: approved_by_id,
        approved_by_name_snapshot: approved_by_name,
        recorded_late: false,
        original_variance_session_key: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    let inserted = movements::insert_cash_movement(db, movement_doc).await?;

    // If movement is associated with an active session, update session totals
    if let Some(session_key) = &body.session_key {
        sessions::increment_session_movements(db, session_key, body.movement_type, body.amount_cents).await?;
    }

    // Update drawer ledger balance (Option A: drawer balance survives across sessions)
    let delta = match body.movement_type {
        CashMovementType::PayIn => body.amount_cents,
        CashMovementType::PayOutExpense
        | CashMovementType::PayOutSupplier
        | CashMovementType::OwnerWithdrawal
        | CashMovementType::CashDrop => -body.amount_cents,
        CashMovementType::DrawerOpenNoSale => 0,
    };

    if delta != 0 {
        drawer::update_drawer_balance(db, &body.terminal_id, delta).await?;
    }

    Ok(inserted.into_cash_movement())
}

pub async fn list_movements(
    db: &Db,
    query: CashMovementListQuery,
) -> AppResult<CashMovementListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).min(200);
    let (docs, total) = movements::list_cash_movements(db, &query).await?;
    let total_pages = (total + limit - 1) / limit;

    Ok(CashMovementListResponse {
        movements: docs.into_iter().map(|d| d.into_cash_movement()).collect(),
        total,
        page,
        limit,
        total_pages,
    })
}

pub async fn get_movement(db: &Db, key: &str) -> AppResult<CashMovement> {
    let doc = movements::find_cash_movement_by_key(db, key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Cash movement not found", codes::CASH_MOVEMENT_NOT_FOUND))?;
    Ok(doc.into_cash_movement())
}
