use super::super::movement;
use crate::clients::db::Db;
use crate::clients::uow::Unit;
use crate::core::constants::codes;
use crate::core::constants::prefixes;
use crate::core::error::AppError;
use crate::core::error::AppResult;
use crate::core::id::generate_id;
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::sessions::ActiveSessionsSummaryResponse;
use crate::domain::sessions::CashMovement;
use crate::domain::sessions::CashMovementType;
use crate::domain::sessions::CloseSessionRequest;
use crate::domain::sessions::ConvertVarianceRequest;
use crate::domain::sessions::OpenSessionRequest;
use crate::domain::sessions::RegisterSession;
use crate::domain::sessions::ReopenSessionRequest;
use crate::domain::sessions::SessionListQuery;
use crate::domain::sessions::SessionListResponse;
use crate::domain::sessions::SessionStatus;
use crate::domain::sessions::SubmitBlindCountRequest;
use crate::domain::sessions::VARIANCE_TOLERANCE_CENTS;
use crate::domain::settings::ShiftMode;
use crate::domain::settings::ShiftPolicy;
use crate::domain::users::Role;
use crate::modules::cash_categories;
use crate::modules::sequences;
use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::model::DrawerStateDocument;
use crate::modules::sessions::model::RegisterSessionDocument;
use crate::modules::sessions::repository::drawer;
use crate::modules::sessions::repository::movements;
use crate::modules::sessions::repository::sessions;
use crate::modules::settings;
use crate::modules::users;
use mongodb::bson::DateTime as BsonDateTime;

/// Fixed drawer ledger `user_key` used for all cash drawers when the shop runs in shared-shop shift mode.
pub const SHOP_DRAWER_KEY: &str = "shop";

#[must_use]
fn drawer_key_for_mode(mode: ShiftMode, cashier_id: &str) -> String {
    match mode {
        ShiftMode::Personal => cashier_id.to_string(),
        ShiftMode::SharedShop => SHOP_DRAWER_KEY.to_string(),
    }
}

#[must_use]
fn active_session_conflict_message(
    mode: ShiftMode,
    cashier_name: &str,
    existing: &RegisterSessionDocument,
) -> String {
    match mode {
        ShiftMode::Personal => format!(
            "Cashier '{}' already has active session ({}) open on terminal '{}'",
            cashier_name, existing.session_number, existing.terminal_id
        ),
        ShiftMode::SharedShop => format!(
            "Shop already has active register session ({}) opened by '{}' on terminal '{}'",
            existing.session_number, existing.cashier_name_snapshot, existing.terminal_id
        ),
    }
}

/// Returns the shop's active shift and staff multi-device policy in a single database query.
pub async fn current_shift_policy<'a>(unit: impl Into<Unit<'a>>) -> AppResult<ShiftPolicy> {
    let profile = settings::service::get_shop_profile(unit).await?;
    Ok(ShiftPolicy::from_persisted(
        profile.allow_shared_user_sessions,
        profile.allow_staff_multi_device_sessions,
    ))
}

/// Returns the shop's active shift mode based on persisted shop profile settings.
pub async fn current_shift_mode<'a>(unit: impl Into<Unit<'a>>) -> AppResult<ShiftMode> {
    Ok(current_shift_policy(unit).await?.mode)
}

/// Resolves the `drawer_state.user_key` affected by transactions under the current shift mode.
pub async fn active_drawer_key<'a>(
    unit: impl Into<Unit<'a>>,
    session: Option<&RegisterSessionDocument>,
    fallback_user_id: &str,
) -> AppResult<String> {
    let mode = current_shift_mode(unit).await?;
    let cashier_id = match session {
        Some(s) => s.cashier_id.as_str(),
        None => fallback_user_id,
    };
    Ok(drawer_key_for_mode(mode, cashier_id))
}

/// Backs out of a started close: a counted ("closing") shift goes back to open (or reopened, if it
/// had been reopened) and the recorded count is cleared. Open and reopened shifts are left alone.
pub async fn cancel_close(db: &Db, session_key: &str) -> AppResult<RegisterSession> {
    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    match session.status {
        SessionStatus::Open | SessionStatus::Reopened => return Ok(session.into_register_session()),
        SessionStatus::Closed => {
            return Err(AppError::conflict(
                codes::SESSION_ALREADY_CLOSED,
                "Cannot cancel closing of an already closed session",
            ));
        }
        SessionStatus::Closing => {}
    }

    session.status = if session.reopened_at.is_some() {
        SessionStatus::Reopened
    } else {
        SessionStatus::Open
    };
    session.closing_cashier_id = None;
    session.closing_cashier_name_snapshot = None;
    session.expected_cash_cents = None;
    session.counted_cash_cents = None;
    session.cash_variance_cents = None;
    session.closing_denominations = None;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| {
            AppError::conflict(
                codes::VERSION_CONFLICT,
                "Register session was modified concurrently; please retry",
            )
        })?;

    Ok(updated.into_register_session())
}

/// Closes the session.
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
    let safe_deposit = body.cash_drop_to_safe_cents.unwrap_or_default();
    if safe_deposit < 0 {
        return Err(AppError::validation(
            "cashDropToSafeCents cannot be negative",
        ));
    }

    if body.closing_float_cents + safe_deposit != body.counted_cash_cents {
        return Err(AppError::validation(format!(
            "Closing float ({} cents) + safe deposit ({} cents) must equal counted cash ({} cents)",
            body.closing_float_cents, safe_deposit, body.counted_cash_cents
        )));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

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
        let missing_reason = match body.variance_reason.as_deref() {
            Some(reason) => reason.trim().is_empty(),
            None => true,
        };
        if missing_reason {
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

    let mut tx = db.begin().await?;
    let updated = sessions::update_session(&mut tx, &session)
        .await?
        .ok_or_else(|| {
            AppError::conflict(
                codes::VERSION_CONFLICT,
                "Register session was modified concurrently; please retry",
            )
        })?;

    // Update drawer ledger: retains the specified closing float for next shift
    let drawer_key = active_drawer_key(&mut tx, Some(&updated), &updated.cashier_id).await?;
    drawer::upsert_drawer_state(
        &mut tx,
        DrawerStateDocument {
            id: None,
            key: drawer_key.clone(),
            user_key: drawer_key,
            current_balance_cents: body.closing_float_cents,
            last_session_key: Some(updated.key.clone()),
            last_closing_float_cents: body.closing_float_cents,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        },
    )
    .await?;
    tx.commit().await?;

    Ok(updated.into_register_session())
}

/// Records a shift's cash variance as a cash movement to a named recipient.
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
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    let Some(variance) = session.cash_variance_cents else {
        return Err(AppError::conflict(
            codes::VARIANCE_APPROVAL_REQUIRED,
            "Cannot convert variance: session has no recorded cash variance",
        ));
    };
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
        direction: CashMovementType::OwnerWithdrawal.direction(),
        amount_cents: shortage_amount,
        session_key: Some(session.key.clone()),
        terminal_id: session.terminal_id.clone(),
        category_key: cash_categories::service::other_category_key(
            CashMovementType::OwnerWithdrawal,
        ),
        custom_label: None,
        payee_key: None,
        payee_name: Some(body.recipient),
        period_start: None,
        period_end: None,
        reference_no: None,
        note: Some(format!(
            "Late-recorded owner withdrawal from variance: {}",
            body.reason
        )),
        recorded_by_user_id,
        recorded_by_name_snapshot: recorded_by_name,
        approved_by_user_id: Some(approver.id),
        approved_by_name_snapshot: Some(approver.name),
        approval_reason: None,
        reverses_key: None,
        recorded_late: true,
        original_variance_session_key: Some(session.key.clone()),
        receipt_key: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    let mut tx = db.begin().await?;
    let inserted = movements::insert_cash_movement(&mut tx, movement_doc).await?;

    // Update session: record owner withdrawal, clear variance shortage, and flag late movements
    session.total_owner_withdrawals_cents += shortage_amount;
    session.expected_cash_cents = Some(session.calculate_expected_cash());
    session.cash_variance_cents = Some(0);
    session.has_late_movements = true;
    sessions::update_session(&mut tx, &session)
        .await?
        .ok_or_else(|| {
            AppError::conflict(
                codes::VERSION_CONFLICT,
                "Register session was modified concurrently; please retry",
            )
        })?;
    tx.commit().await?;

    Ok(movement::to_movements(db, vec![inserted]).await?.remove(0))
}

/// What the drawer should hold when a shift opens: its running balance. A terminal that has never
/// held cash has nothing to compare the count with, so the count itself stands.
pub(crate) fn expected_opening_float(
    state: Option<&DrawerStateDocument>,
    counted_cents: i64,
) -> i64 {
    state.map_or(counted_cents, |s| s.current_balance_cents)
}

/// Returns active sessions across `open`, `closing`, and `reopened` statuses plus the summed
/// balance across all `drawer_state` rows, used by Settings UI before toggling shift mode.
pub async fn get_active_sessions_summary(db: &Db) -> AppResult<ActiveSessionsSummaryResponse> {
    let docs = sessions::list_active_sessions(db).await?;
    let drawer_states = drawer::list_drawer_states(db).await?;
    let drawer_balance_total_cents = drawer_states.iter().map(|s| s.current_balance_cents).sum();
    Ok(ActiveSessionsSummaryResponse {
        active_sessions: docs
            .into_iter()
            .map(RegisterSessionDocument::into_register_session)
            .collect(),
        drawer_balance_total_cents,
    })
}

/// Returns the active session for `cashier_id` (`ShiftMode::Personal`) or the shop (`ShiftMode::SharedShop`).
pub async fn get_active_session_for_cashier(
    db: &Db,
    cashier_id: &str,
) -> AppResult<Option<RegisterSession>> {
    let session = resolve_active_session(db, cashier_id).await?;
    Ok(session.map(RegisterSessionDocument::into_register_session))
}

/// Returns the session.
pub async fn get_session(db: &Db, key: &str) -> AppResult<RegisterSession> {
    let session = sessions::find_session_by_id_or_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;
    Ok(session.into_register_session())
}

/// Lists the sessions.
pub async fn list_sessions(db: &Db, query: SessionListQuery) -> AppResult<SessionListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).min(100);
    let (docs, total) = sessions::list_sessions(db, &query).await?;
    let total_pages = total.div_ceil(limit);

    let sessions = docs
        .into_iter()
        .map(RegisterSessionDocument::into_register_session)
        .collect();

    Ok(SessionListResponse {
        sessions,
        total,
        page,
        limit,
        total_pages,
    })
}

/// Opens a register session (one per cashier in `Personal` mode; one shop-wide in `SharedShop` mode).
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

    let mode = current_shift_mode(db).await?;
    let existing = match mode {
        ShiftMode::Personal => sessions::find_active_session_for_cashier(db, &cashier_id).await?,
        ShiftMode::SharedShop => sessions::find_active_session_for_shop(db).await?,
    };
    if let Some(existing) = existing {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            active_session_conflict_message(mode, &cashier_name, &existing),
        ));
    }

    let drawer_key = drawer_key_for_mode(mode, &cashier_id);
    let drawer_state = drawer::get_drawer_state(db, &drawer_key).await?;
    let expected_opening_float =
        expected_opening_float(drawer_state.as_ref(), body.opening_float_cents);

    let discrepancy = body.opening_float_cents - expected_opening_float;
    let mut approved_by_id = None;
    let mut approved_by_name = None;

    if discrepancy != 0 {
        let missing_reason = match body.discrepancy_reason.as_deref() {
            Some(reason) => reason.trim().is_empty(),
            None => true,
        };
        if missing_reason {
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
        cashier_id: cashier_id.clone(),
        cashier_name_snapshot: cashier_name.clone(),
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
        deleted_at: None,
        updated_by_device: None,
    };

    let mut tx = db.begin().await?;
    let tx_mode = current_shift_mode(&mut tx).await?;
    if tx_mode != mode {
        return Err(AppError::conflict(
            codes::VERSION_CONFLICT,
            "Shift mode changed while opening session; please retry",
        ));
    }
    let tx_existing = match tx_mode {
        ShiftMode::Personal => {
            sessions::find_active_session_for_cashier(&mut tx, &cashier_id).await?
        }
        ShiftMode::SharedShop => sessions::find_active_session_for_shop(&mut tx).await?,
    };
    if let Some(existing) = tx_existing {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            active_session_conflict_message(tx_mode, &cashier_name, &existing),
        ));
    }

    let inserted = sessions::insert_session(&mut tx, session_doc).await?;
    let tx_drawer_state = drawer::get_drawer_state(&mut tx, &drawer_key).await?;
    let last_closing_float_cents = match tx_drawer_state {
        Some(s) => s.last_closing_float_cents,
        None => 0,
    };

    // Update drawer ledger balance to match opening float
    drawer::upsert_drawer_state(
        &mut tx,
        DrawerStateDocument {
            id: None,
            key: drawer_key.clone(),
            user_key: drawer_key,
            current_balance_cents: body.opening_float_cents,
            last_session_key: Some(inserted.key.clone()),
            last_closing_float_cents,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        },
    )
    .await?;
    tx.commit().await?;

    Ok(inserted.into_register_session())
}

/// Reopens the session.
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
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    if session.status != SessionStatus::Closed {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            "Only closed sessions can be reopened",
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

    let expected_cash = session.calculate_expected_cash();

    let mut tx = db.begin().await?;
    let mode = current_shift_mode(&mut tx).await?;
    let conflict = match mode {
        ShiftMode::Personal => {
            sessions::find_active_session_for_cashier(&mut tx, &session.cashier_id).await?
        }
        ShiftMode::SharedShop => sessions::find_active_session_for_shop(&mut tx).await?,
    };
    if let Some(existing) = conflict
        && existing.key != session.key
    {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            match mode {
                ShiftMode::Personal => format!(
                    "Cannot reopen session: cashier already has an active register session ({}) open",
                    existing.session_number
                ),
                ShiftMode::SharedShop => format!(
                    "Cannot reopen session: shop already has an active register session ({}) open",
                    existing.session_number
                ),
            },
        ));
    }

    let updated = sessions::update_session(&mut tx, &session)
        .await?
        .ok_or_else(|| {
            AppError::conflict(
                codes::VERSION_CONFLICT,
                "Register session was modified concurrently; please retry",
            )
        })?;

    // Restore drawer ledger balance to expected cash for resumed operation
    let drawer_key = drawer_key_for_mode(mode, &updated.cashier_id);
    drawer::upsert_drawer_state(
        &mut tx,
        DrawerStateDocument {
            id: None,
            key: drawer_key.clone(),
            user_key: drawer_key,
            current_balance_cents: expected_cash,
            last_session_key: Some(updated.key.clone()),
            last_closing_float_cents: updated.opening_float_cents,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        },
    )
    .await?;
    tx.commit().await?;

    Ok(updated.into_register_session())
}

/// Resolves the active session for `cashier_id`: their own in `Personal` mode, or the shop's in `SharedShop` mode.
pub async fn resolve_active_session(
    db: &Db,
    cashier_id: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    match current_shift_mode(db).await? {
        ShiftMode::Personal => sessions::find_active_session_for_cashier(db, cashier_id).await,
        ShiftMode::SharedShop => sessions::find_active_session_for_shop(db).await,
    }
}

/// Resolves the session for recording sales, payments, or credit notes, enforcing mid-count locks (`409 SESSION_CLOSING`).
pub async fn resolve_writable_session(
    db: &Db,
    requested_session_key: Option<&str>,
    cashier_id: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    let session = match requested_session_key {
        Some(key) => {
            let doc = sessions::find_session_by_key(db, key)
                .await?
                .ok_or_else(|| {
                    AppError::not_found_with_code(
                        "Register session not found",
                        codes::SESSION_NOT_FOUND,
                    )
                })?;
            Some(doc)
        }
        None => resolve_active_session(db, cashier_id).await?,
    };
    let Some(session) = session else {
        return Ok(None);
    };
    match session.status {
        SessionStatus::Open | SessionStatus::Reopened => Ok(Some(session)),
        SessionStatus::Closing => Err(AppError::conflict(
            codes::SESSION_CLOSING,
            "Cash count is in progress for this register session",
        )),
        SessionStatus::Closed => Err(AppError::conflict(
            codes::SESSION_ALREADY_CLOSED,
            "Cannot record transactions against a closed register session",
        )),
    }
}

/// Submits the blind count and locks the shift in `closing` status until closed or canceled.
pub async fn submit_blind_count(
    db: &Db,
    session_key: &str,
    cashier_id: String,
    cashier_name: String,
    body: SubmitBlindCountRequest,
) -> AppResult<RegisterSession> {
    if body.counted_cash_cents < 0 {
        return Err(AppError::validation("countedCashCents cannot be negative"));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    if session.status != SessionStatus::Open
        && session.status != SessionStatus::Closing
        && session.status != SessionStatus::Reopened
    {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_CLOSED,
            "Cannot submit count for an already closed session",
        ));
    }

    let expected_cash = session.calculate_expected_cash();
    let variance = body.counted_cash_cents - expected_cash;

    session.closing_cashier_id = Some(cashier_id);
    session.closing_cashier_name_snapshot = Some(cashier_name);
    session.expected_cash_cents = Some(expected_cash);
    session.counted_cash_cents = Some(body.counted_cash_cents);
    session.cash_variance_cents = Some(variance);
    session.closing_denominations = body.closing_denominations;
    session.status = SessionStatus::Closing;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| {
            AppError::conflict(
                codes::VERSION_CONFLICT,
                "Register session was modified concurrently; please retry",
            )
        })?;

    Ok(updated.into_register_session())
}
