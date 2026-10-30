use super::expected_opening_float::expected_opening_float;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::sessions::{OpenSessionRequest, RegisterSession, SessionStatus};

use crate::modules::sessions::model::{DrawerStateDocument, RegisterSessionDocument};
use crate::modules::sessions::repository::{drawer, sessions};
use crate::modules::{sequences, users};
use mongodb::bson::DateTime as BsonDateTime;

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
    if let Some(existing) =
        sessions::find_active_session_for_terminal(db, &body.terminal_id).await?
    {
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
    let expected_opening_float =
        expected_opening_float(drawer_state.as_ref(), body.opening_float_cents);

    let discrepancy = body.opening_float_cents - expected_opening_float;
    let mut approved_by_id = None;
    let mut approved_by_name = None;

    if discrepancy != 0 {
        if body
            .discrepancy_reason
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
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
