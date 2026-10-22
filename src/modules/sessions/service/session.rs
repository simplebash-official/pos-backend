// Register session lifecycle service: open, blind count, close, reopen, and variance conversion.

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
            CashMovement, CashMovementType, CloseSessionRequest, ConvertVarianceRequest,
            OpenSessionRequest, RegisterSession, ReopenSessionRequest, SessionListQuery,
            SessionListResponse, SessionStatus, SubmitBlindCountRequest,
        },
        users::Role,
    },
    modules::{
        sequences,
        sessions::{
            model::{CashMovementDocument, DrawerStateDocument, RegisterSessionDocument},
            repository::{drawer, movements, sessions},
        },
        users,
    },
};

/// Small variance tolerance in cents (e.g. Rs 5.00 / 500 cents) that cashiers can submit with note only.
pub const VARIANCE_TOLERANCE_CENTS: i64 = 500;

pub async fn open_session(
    db: &Db,
    cashier_id: String,
    cashier_name: String,
    body: OpenSessionRequest,
    device_id: Option<String>,
) -> AppResult<RegisterSession> {
    if body.terminal_id.trim().is_empty() {
        return Err(AppError::validation("terminalId is required"));
    }
    if body.opening_float_cents < 0 {
        return Err(AppError::validation("openingFloatCents cannot be negative"));
    }

    // Guard: only 1 open session per terminal
    if let Some(existing) = sessions::find_active_session_for_terminal(db, &body.terminal_id).await? {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            format!(
                "An active register session ({}) is already open on terminal '{}'",
                existing.session_number, body.terminal_id
            ),
        ));
    }

    // Guard: only 1 open session per cashier
    if let Some(existing) = sessions::find_active_session_for_cashier(db, &cashier_id).await? {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            format!(
                "Cashier '{}' already has active session ({}) open on terminal '{}'",
                cashier_name, existing.session_number, existing.terminal_id
            ),
        ));
    }

    // Ledger check: inspect drawer balance from previous close / outside movements
    let drawer_state = drawer::get_drawer_state(db, &body.terminal_id).await?;
    let expected_opening_float = drawer_state
        .as_ref()
        .map(|s| s.current_balance_cents)
        .unwrap_or(body.opening_float_cents);

    let discrepancy = body.opening_float_cents - expected_opening_float;
    let mut approved_by_id = None;
    let mut approved_by_name = None;

    if discrepancy != 0 {
        if body.discrepancy_reason.as_deref().unwrap_or("").trim().is_empty() {
            return Err(AppError::validation_with_code(
                format!(
                    "Counted float ({} cents) differs from drawer balance ({} cents). A discrepancy reason is required.",
                    body.opening_float_cents, expected_opening_float
                ),
                codes::VARIANCE_APPROVAL_REQUIRED,
            ));
        }

        let pin = body.manager_pin.as_deref().ok_or_else(|| {
            AppError::forbidden_with_code(
                "Manager PIN required to approve opening float discrepancy",
                codes::VARIANCE_APPROVAL_REQUIRED,
            )
        })?;

        let approver = users::service::verify_user_pin(db, pin, Some("manager"))
            .await?
            .ok_or_else(|| {
                AppError::unauthorized_with_code("Invalid manager PIN", codes::INVALID_PIN)
            })?;

        approved_by_id = Some(approver.id);
        approved_by_name = Some(approver.name);
    }

    let reservation = sequences::service::reserve_sequence(
        db,
        "session".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id,
        },
    )
    .await?;

    let session_number = format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    );

    let now = BsonDateTime::now();
    let session_doc = RegisterSessionDocument {
        id: None,
        key: generate_id(prefixes::REGISTER_SESSION),
        session_number,
        terminal_id: body.terminal_id.clone(),
        cashier_id,
        cashier_name_snapshot: cashier_name,
        status: SessionStatus::Open,
        opened_at: now,
        opening_float_cents: body.opening_float_cents,
        opening_denominations: body.opening_denominations,
        opening_note: body.opening_note,
        expected_opening_float_cents: Some(expected_opening_float),
        opening_discrepancy_cents: discrepancy,
        opening_discrepancy_reason: body.discrepancy_reason,
        opening_approved_by_id: approved_by_id,
        opening_approved_by_name: approved_by_name,
        closed_at: None,
        closing_cashier_id: None,
        closing_cashier_name_snapshot: None,
        expected_cash_cents: None,
        counted_cash_cents: None,
        cash_variance_cents: None,
        closing_denominations: None,
        closing_note: None,
        closing_float_cents: None,
        cash_drop_to_safe_cents: None,
        variance_reason: None,
        variance_approved_by_id: None,
        variance_approved_by_name: None,
        reopened_at: None,
        reopened_by_id: None,
        reopened_by_name: None,
        reopened_reason: None,
        total_cash_sales_cents: 0,
        total_card_sales_cents: 0,
        total_online_sales_cents: 0,
        total_credit_sales_cents: 0,
        total_cash_refunds_cents: 0,
        total_pay_in_cents: 0,
        total_pay_out_cents: 0,
        total_owner_withdrawals_cents: 0,
        total_cash_drops_cents: 0,
        has_late_movements: false,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    let inserted = sessions::insert_session(db, session_doc).await?;

    // Update drawer ledger balance to match opening float
    drawer::upsert_drawer_state(
        db,
        DrawerStateDocument {
            terminal_id: body.terminal_id,
            current_balance_cents: body.opening_float_cents,
            last_session_key: Some(inserted.key.clone()),
            last_closing_float_cents: drawer_state
                .map(|s| s.last_closing_float_cents)
                .unwrap_or(0),
            updated_at: now,
        },
    )
    .await?;

    Ok(inserted.into_register_session())
}

pub async fn submit_blind_count(
    db: &Db,
    session_key: &str,
    body: SubmitBlindCountRequest,
) -> AppResult<RegisterSession> {
    if body.counted_cash_cents < 0 {
        return Err(AppError::validation("countedCashCents cannot be negative"));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND))?;

    if session.status != SessionStatus::Open && session.status != SessionStatus::Closing && session.status != SessionStatus::Reopened {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_CLOSED,
            "Cannot submit count for an already closed session",
        ));
    }

    let expected_cash = session.calculate_expected_cash();
    let variance = body.counted_cash_cents - expected_cash;

    session.expected_cash_cents = Some(expected_cash);
    session.counted_cash_cents = Some(body.counted_cash_cents);
    session.cash_variance_cents = Some(variance);
    session.closing_denominations = body.closing_denominations;
    session.status = SessionStatus::Closing;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| AppError::internal("Failed to save blind count on session"))?;

    Ok(updated.into_register_session())
}

pub async fn close_session(
    db: &Db,
    session_key: &str,
    cashier_id: String,
    cashier_name: String,
    body: CloseSessionRequest,
) -> AppResult<RegisterSession> {
    if body.counted_cash_cents < 0 {
        return Err(AppError::validation("countedCashCents cannot be negative"));
    }
    if body.closing_float_cents < 0 {
        return Err(AppError::validation("closingFloatCents cannot be negative"));
    }
    let safe_deposit = body.cash_drop_to_safe_cents.unwrap_or(0);
    if safe_deposit < 0 {
        return Err(AppError::validation("cashDropToSafeCents cannot be negative"));
    }

    if body.closing_float_cents + safe_deposit != body.counted_cash_cents {
        return Err(AppError::validation(format!(
            "Closing float ({} cents) + safe deposit ({} cents) must equal counted cash ({} cents)",
            body.closing_float_cents, safe_deposit, body.counted_cash_cents
        )));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND))?;

    if session.status == SessionStatus::Closed {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_CLOSED,
            "Register session is already closed",
        ));
    }

    let expected_cash = session.calculate_expected_cash();
    let variance = body.counted_cash_cents - expected_cash;
    let mut variance_approved_by_id = None;
    let mut variance_approved_by_name = None;

    if variance != 0 {
        if body.variance_reason.as_deref().unwrap_or("").trim().is_empty() {
            return Err(AppError::validation_with_code(
                format!(
                    "Cash variance of {} cents requires a variance explanation reason",
                    variance
                ),
                codes::VARIANCE_APPROVAL_REQUIRED,
            ));
        }

        // If variance is outside small tolerance, require manager PIN approval
        if variance.abs() > VARIANCE_TOLERANCE_CENTS {
            let pin = body.manager_pin.as_deref().ok_or_else(|| {
                AppError::forbidden_with_code(
                    "Manager PIN is required to approve cash variance exceeding tolerance",
                    codes::VARIANCE_APPROVAL_REQUIRED,
                )
            })?;

            let approver = users::service::verify_user_pin(db, pin, Some("manager"))
                .await?
                .ok_or_else(|| {
                    AppError::unauthorized_with_code("Invalid manager PIN", codes::INVALID_PIN)
                })?;

            variance_approved_by_id = Some(approver.id);
            variance_approved_by_name = Some(approver.name);
        }
    }

    let now = BsonDateTime::now();
    session.closed_at = Some(now);
    session.closing_cashier_id = Some(cashier_id);
    session.closing_cashier_name_snapshot = Some(cashier_name);
    session.expected_cash_cents = Some(expected_cash);
    session.counted_cash_cents = Some(body.counted_cash_cents);
    session.cash_variance_cents = Some(variance);
    session.closing_denominations = body.closing_denominations;
    session.closing_note = body.closing_note;
    session.closing_float_cents = Some(body.closing_float_cents);
    session.cash_drop_to_safe_cents = Some(safe_deposit);
    session.variance_reason = body.variance_reason;
    session.variance_approved_by_id = variance_approved_by_id;
    session.variance_approved_by_name = variance_approved_by_name;
    session.status = SessionStatus::Closed;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| AppError::internal("Failed to update register session status to closed"))?;

    // Update drawer ledger: retains the specified closing float for next shift
    drawer::upsert_drawer_state(
        db,
        DrawerStateDocument {
            terminal_id: session.terminal_id.clone(),
            current_balance_cents: body.closing_float_cents,
            last_session_key: Some(session.key.clone()),
            last_closing_float_cents: body.closing_float_cents,
            updated_at: now,
        },
    )
    .await?;

    Ok(updated.into_register_session())
}

pub async fn reopen_session(
    db: &Db,
    session_key: &str,
    user_id: String,
    user_name: String,
    user_role: Option<Role>,
    body: ReopenSessionRequest,
) -> AppResult<RegisterSession> {
    if user_role != Some(Role::Admin) && user_role != Some(Role::Manager) {
        return Err(AppError::forbidden_with_code(
            "Only managers and admins can reopen a closed session",
            codes::PERMISSION_DENIED,
        ));
    }
    if body.reason.trim().is_empty() {
        return Err(AppError::validation("Reopen reason is required"));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND))?;

    if session.status != SessionStatus::Closed {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            "Only closed sessions can be reopened",
        ));
    }

    // Ensure no other active session has been opened on this terminal
    if let Some(existing) = sessions::find_active_session_for_terminal(db, &session.terminal_id).await?
        && existing.key != session.key
    {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            format!(
                "Cannot reopen session: an active register session ({}) is already open on terminal '{}'",
                existing.session_number, session.terminal_id
            ),
        ));
    }

    // Ensure this cashier does not already have another active session
    if let Some(existing) = sessions::find_active_session_for_cashier(db, &session.cashier_id).await?
        && existing.key != session.key
    {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            format!(
                "Cannot reopen session: cashier already has an active register session ({}) open",
                existing.session_number
            ),
        ));
    }

    let now = BsonDateTime::now();
    session.status = SessionStatus::Reopened;
    session.closed_at = None;
    session.closing_cashier_id = None;
    session.closing_cashier_name_snapshot = None;
    session.expected_cash_cents = None;
    session.counted_cash_cents = None;
    session.cash_variance_cents = None;
    session.closing_denominations = None;
    session.closing_float_cents = None;
    session.cash_drop_to_safe_cents = None;
    session.variance_reason = None;
    session.variance_approved_by_id = None;
    session.variance_approved_by_name = None;
    session.closing_note = None;
    session.reopened_at = Some(now);
    session.reopened_by_id = Some(user_id);
    session.reopened_by_name = Some(user_name);
    session.reopened_reason = Some(body.reason);

    // Restore drawer ledger balance to expected cash for resumed operation
    let expected_cash = session.calculate_expected_cash();
    drawer::upsert_drawer_state(
        db,
        DrawerStateDocument {
            terminal_id: session.terminal_id.clone(),
            current_balance_cents: expected_cash,
            last_session_key: Some(session.key.clone()),
            last_closing_float_cents: session.opening_float_cents,
            updated_at: now,
        },
    )
    .await?;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| AppError::internal("Failed to reopen register session"))?;

    Ok(updated.into_register_session())
}

pub async fn convert_variance(
    db: &Db,
    session_key: &str,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: ConvertVarianceRequest,
    device_id: Option<String>,
) -> AppResult<CashMovement> {
    if body.reason.trim().is_empty() {
        return Err(AppError::validation("Reason is required"));
    }
    if body.recipient.trim().is_empty() {
        return Err(AppError::validation("Recipient is required"));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND))?;

    let variance = session.cash_variance_cents.unwrap_or(0);
    if variance >= 0 {
        return Err(AppError::conflict(
            codes::VARIANCE_APPROVAL_REQUIRED,
            "Cannot convert variance: session does not have a cash shortage",
        ));
    }

    let shortage_amount = variance.abs();

    // Verify PIN: if shortage amount exceeds threshold, require owner/admin specifically
    let approver = users::service::verify_user_pin(db, &body.manager_pin, Some("manager"))
        .await?
        .ok_or_else(|| AppError::unauthorized_with_code("Invalid PIN", codes::INVALID_PIN))?;

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
        movement_type: CashMovementType::OwnerWithdrawal,
        amount_cents: shortage_amount,
        session_key: Some(session.key.clone()),
        terminal_id: session.terminal_id.clone(),
        reason: format!("Late-recorded owner withdrawal from variance: {}", body.reason),
        recipient_or_source: Some(body.recipient),
        supplier_key: None,
        recorded_by_user_id,
        recorded_by_name_snapshot: recorded_by_name,
        approved_by_user_id: Some(approver.id),
        approved_by_name_snapshot: Some(approver.name),
        recorded_late: true,
        original_variance_session_key: Some(session.key.clone()),
        version: 1,
        created_at: now,
        updated_at: now,
    };

    let inserted = movements::insert_cash_movement(db, movement_doc).await?;

    // Update session: record owner withdrawal, clear variance shortage, and flag late movements
    session.total_owner_withdrawals_cents += shortage_amount;
    session.expected_cash_cents = Some(session.calculate_expected_cash());
    session.cash_variance_cents = Some(0);
    session.has_late_movements = true;
    sessions::update_session(db, &session).await?;

    Ok(inserted.into_cash_movement())
}

/// If a session's `cashier_name_snapshot` is empty (e.g. from an older session opened
/// when cashier names were not propagated), resolve it from `users::service::get_user`.
pub async fn populate_missing_cashier_name(db: &Db, session: &mut RegisterSession) {
    if session.cashier_name_snapshot.trim().is_empty()
        && let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(&session.cashier_id)
            && let Ok(user) = users::service::get_user(db, oid).await {
                let trimmed = user.name.trim();
                if !trimmed.is_empty() {
                    session.cashier_name_snapshot = trimmed.to_string();
                }
            }
    if let Some(ref closing_id) = session.closing_cashier_id
        && session
            .closing_cashier_name_snapshot
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
            && let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(closing_id)
                && let Ok(user) = users::service::get_user(db, oid).await {
                    let trimmed = user.name.trim();
                    if !trimmed.is_empty() {
                        session.closing_cashier_name_snapshot = Some(trimmed.to_string());
                    }
                }
}

pub async fn get_session(db: &Db, key: &str) -> AppResult<RegisterSession> {
    let session = sessions::find_session_by_id_or_key(db, key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND))?;
    let mut reg = session.into_register_session();
    populate_missing_cashier_name(db, &mut reg).await;
    Ok(reg)
}

pub async fn get_active_session_for_terminal(
    db: &Db,
    terminal_id: &str,
) -> AppResult<Option<RegisterSession>> {
    let session = sessions::find_active_session_for_terminal(db, terminal_id).await?;
    match session {
        Some(s) => {
            let mut reg = s.into_register_session();
            populate_missing_cashier_name(db, &mut reg).await;
            Ok(Some(reg))
        }
        None => Ok(None),
    }
}

pub async fn get_active_session_for_cashier(
    db: &Db,
    cashier_id: &str,
) -> AppResult<Option<RegisterSession>> {
    let session = sessions::find_active_session_for_cashier(db, cashier_id).await?;
    match session {
        Some(s) => {
            let mut reg = s.into_register_session();
            populate_missing_cashier_name(db, &mut reg).await;
            Ok(Some(reg))
        }
        None => Ok(None),
    }
}

pub async fn list_sessions(
    db: &Db,
    query: SessionListQuery,
) -> AppResult<SessionListResponse> {
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

pub async fn resolve_active_session(
    db: &Db,
    terminal_id: Option<&str>,
    cashier_id: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    let profile = crate::modules::settings::service::get_shop_profile(db, None).await?;
    let term = terminal_id.map(str::trim).filter(|t| !t.is_empty());

    if profile.allow_shared_user_sessions {
        if let Some(t) = term
            && let Some(s) = sessions::find_active_session_for_terminal(db, t).await?
        {
            return Ok(Some(s));
        }
        sessions::find_active_session_for_cashier(db, cashier_id).await
    } else {
        if let Some(t) = term {
            sessions::find_active_session_for_terminal(db, t).await
        } else {
            Ok(None)
        }
    }
}
