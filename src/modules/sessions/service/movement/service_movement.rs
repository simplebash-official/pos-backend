use super::prepare_movement::prepare_movement;
use crate::clients::db::Db;
use crate::clients::uow::Tx;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::purchase_receipts::PaymentKind;
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::sessions::{
    CashDirection, CashMovement, CashMovementListQuery, CashMovementListResponse, CashMovementType,
    CreateCashMovementRequest, ReverseCashMovementRequest, SessionStatus,
};
use crate::modules::purchases::service::settlement;
use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::{drawer, movements, sessions};
use crate::modules::{cash_categories, cash_settings, sequences};
use mongodb::bson::DateTime as BsonDateTime;
use std::collections::HashMap;

/// Cash taken to the bank or brought back from it, from `since` (`YYYY-MM-DD`) on, as
/// `(deposited, withdrawn)`. A deposit is a cash drop under any of `deposit_categories`, a withdrawal an
/// add-cash under any of `withdrawal_categories`; reversed movements and their reversals cancel out.
pub(crate) async fn bank_cash_moves(
    db: &Db,
    since: &str,
    deposit_categories: &[String],
    withdrawal_categories: &[String],
) -> AppResult<(i64, i64)> {
    let movements = movements::list_movements_of_types(
        db,
        &[CashMovementType::CashDrop, CashMovementType::AddCash],
        Some(since),
        None,
    )
    .await?;
    let keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = reversed_keys(db, &keys).await?;
    let (mut deposited, mut withdrawn) = (0, 0);
    for m in movements
        .iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
    {
        match m.movement_type {
            CashMovementType::CashDrop if deposit_categories.contains(&m.category_key) => {
                deposited += m.amount_cents
            }
            CashMovementType::AddCash if withdrawal_categories.contains(&m.category_key) => {
                withdrawn += m.amount_cents;
            }
            _ => {}
        }
    }
    Ok((deposited, withdrawn))
}

/// The cash the drawer holds right now: the open shift's expected cash, or, between shifts, the
/// drawer's own running balance.
pub(crate) async fn cash_in_drawer(
    db: &Db,
    session_key: Option<&str>,
    terminal_id: &str,
) -> AppResult<i64> {
    if let Some(session_key) = session_key {
        let session = sessions::find_session_by_key(db, session_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code(
                    "Register session not found",
                    codes::SESSION_NOT_FOUND,
                )
            })?;
        if !matches!(
            session.status,
            SessionStatus::Open | SessionStatus::Reopened
        ) {
            return Err(AppError::validation_with_code(
                "This shift is not open, so cash cannot be recorded on it.",
                codes::NO_ACTIVE_SESSION,
            ));
        }
        Ok(session.calculate_expected_cash())
    } else {
        Ok(drawer::get_drawer_state(db, terminal_id)
            .await?
            .map_or(0, |state| state.current_balance_cents))
    }
}

/// Live category names for a batch of movement rows.
pub(crate) async fn category_names(
    db: &Db,
    docs: &[CashMovementDocument],
) -> AppResult<HashMap<String, String>> {
    let mut keys: Vec<String> = docs.iter().map(|d| d.category_key.clone()).collect();
    keys.sort_unstable();
    keys.dedup();
    cash_categories::service::display_names(db, &keys).await
}

/// Trims an optional text field; blank means absent, too long is refused.
pub(super) fn clean_text(
    label: &str,
    value: Option<&str>,
    max: usize,
) -> AppResult<Option<String>> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > max {
        return Err(AppError::validation(format!(
            "{label} can have up to {max} letters."
        )));
    }
    Ok(Some(text.to_string()))
}

/// Cash the shop holds in all its drawers right now (the sum of every terminal's running balance).
pub(crate) async fn drawer_cash_total(db: &Db) -> AppResult<i64> {
    Ok(drawer::list_drawer_states(db)
        .await?
        .iter()
        .map(|s| s.current_balance_cents)
        .sum())
}

/// How a movement changes the cash in the drawer: added cash raises it, every payout and drop lowers it.
pub(super) fn drawer_delta(movement_type: CashMovementType, amount_cents: i64) -> i64 {
    use CashMovementType as T;
    match movement_type {
        T::AddCash => amount_cents,
        T::PettyExpense
        | T::RentAndBills
        | T::StaffPayment
        | T::SupplierPayment
        | T::OwnerWithdrawal
        | T::CashDrop => -amount_cents,
        T::NoSale => 0,
    }
}

/// Drawer payments that can be business costs (every outgoing kind but owner drawings), between
/// two days, for the expense ledger to read. The ledger keeps only those whose category counts as a cost.
pub(crate) async fn expense_movements(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<CashMovementDocument>> {
    movements::list_movements_of_types(
        db,
        &[
            CashMovementType::PettyExpense,
            CashMovementType::RentAndBills,
            CashMovementType::StaffPayment,
            CashMovementType::SupplierPayment,
        ],
        from,
        to,
    )
    .await
}

/// Returns the movement.
pub async fn get_movement(db: &Db, key: &str) -> AppResult<CashMovement> {
    let doc = movements::find_cash_movement_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Cash movement not found", codes::CASH_MOVEMENT_NOT_FOUND)
        })?;
    one_movement(db, doc).await
}

/// Cash put into the drawer between two days, for the income ledger to read. The ledger keeps
/// only those whose category counts as income (not a float, capital or a loan).
pub(crate) async fn income_movements(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<CashMovementDocument>> {
    movements::list_movements_of_types(db, &[CashMovementType::AddCash], from, to).await
}

/// A page of movements, newest first. Filtering by a category includes everything merged into it.
pub async fn list_movements(
    db: &Db,
    mut query: CashMovementListQuery,
) -> AppResult<CashMovementListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).min(200);
    if let Some(key) = query.category_key.take() {
        query.category_keys =
            Some(cash_categories::service::keys_including_merged(db, &key).await?);
    }
    let (docs, total) = movements::list_cash_movements(db, &query).await?;
    let total_pages = total.div_ceil(limit);

    Ok(CashMovementListResponse {
        movements: to_movements(db, docs).await?,
        total,
        page,
        limit,
        total_pages,
    })
}

/// Movements per category key (including none merged), for ranking the category picker.
pub(crate) async fn movement_counts_by_category(db: &Db) -> AppResult<Vec<(String, i64)>> {
    movements::count_movements_by_category(db).await
}

/// Movements per saved payee key, for ranking the payee list.
pub(crate) async fn movement_counts_by_payee(db: &Db) -> AppResult<Vec<(String, i64)>> {
    movements::count_movements_by_payee(db).await
}

/// Every drawer movement recorded between two days (`YYYY-MM-DD`, UTC, inclusive), reversed ones and their
/// reversals left out, for the cash flow report.
pub(crate) async fn movements_between(
    db: &Db,
    from: &str,
    to: &str,
) -> AppResult<Vec<CashMovementDocument>> {
    let all = [
        CashMovementType::PettyExpense,
        CashMovementType::RentAndBills,
        CashMovementType::StaffPayment,
        CashMovementType::SupplierPayment,
        CashMovementType::OwnerWithdrawal,
        CashMovementType::AddCash,
        CashMovementType::CashDrop,
    ];
    let movements = movements::list_movements_of_types(db, &all, Some(from), Some(to)).await?;
    let keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = reversed_keys(db, &keys).await?;
    Ok(movements
        .into_iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
        .collect())
}

pub(super) const NOTE_MAX: usize = 200;

/// One stored row as the API shape.
pub(super) async fn one_movement(db: &Db, doc: CashMovementDocument) -> AppResult<CashMovement> {
    Ok(to_movements(db, vec![doc]).await?.remove(0))
}

/// One drawer payout, as the balances need it.
#[derive(Debug)]
pub(crate) struct Payout {
    pub payee_key: Option<String>,
    pub category_key: String,
    /// `YYYY-MM-DD` (UTC) the cash was handed over.
    pub day: String,
    pub number: String,
    pub amount_cents: i64,
}

/// Every drawer movement of one type, reversed ones and their reversals left out.
pub(crate) async fn payouts_of_type(
    db: &Db,
    movement_type: CashMovementType,
) -> AppResult<Vec<Payout>> {
    let movements = movements::list_movements_of_types(db, &[movement_type], None, None).await?;
    let keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = reversed_keys(db, &keys).await?;
    Ok(movements
        .into_iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
        .map(|m| Payout {
            day: m.created_at.to_chrono().format("%Y-%m-%d").to_string(),
            payee_key: m.payee_key,
            category_key: m.category_key,
            number: m.movement_number,
            amount_cents: m.amount_cents,
        })
        .collect())
}

/// Records one drawer cash event. Validates the shift, the amount, the category, the fields
/// this type needs and (where required) a manager PIN, then books it on the shift and the drawer.
pub async fn record_movement(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateCashMovementRequest,
    device_id: Option<String>,
) -> AppResult<CashMovement> {
    tracked("cash_movement.recorded", async move {
        let requested = body.settlements.clone();
        if !requested.is_empty() && body.movement_type != CashMovementType::SupplierPayment {
            return Err(AppError::validation(
                "Receipts can only be paid by a supplier payment.",
            ));
        }
        let document = prepare_movement(
            db,
            recorded_by_user_id,
            recorded_by_name,
            body,
            device_id,
            None,
            0,
        )
        .await?;
        // The payee is a resolved supplier by now; its receipts are checked before anything is written.
        let settlements = if requested.is_empty() {
            None
        } else {
            let supplier_key = document
                .payee_key
                .as_deref()
                .ok_or_else(|| AppError::internal("A supplier payment must have a supplier"))?;
            Some(
                settlement::prepare_settlements(
                    db,
                    supplier_key,
                    document.amount_cents,
                    &requested,
                )
                .await?,
            )
        };
        let mut tx = db.begin().await?;
        let inserted = write_movement(&mut tx, document).await?;
        if let Some(prepared) = settlements {
            settlement::write_settlements(
                &mut tx,
                prepared,
                PaymentKind::CashMovement,
                &inserted.key,
            )
            .await?;
        }
        tx.commit().await?;
        one_movement(db, inserted).await
    })
    .await
}

/// Types that move accountable money and need the managed-payout permission on top of the basic one.
#[must_use]
pub fn requires_managed_permission(movement_type: CashMovementType) -> bool {
    matches!(
        movement_type,
        CashMovementType::RentAndBills
            | CashMovementType::StaffPayment
            | CashMovementType::SupplierPayment
    )
}

/// Cancels a movement by recording its opposite; the original row is never edited or deleted. The person
/// who recorded it can undo it inside the shop's undo window (`cash_settings`); after that only someone allowed to reverse
/// (`can_reverse`) can, and must say why. Reversing twice is refused: the reversal's key is derived from
/// the original's, so two devices undoing the same movement offline are the same row.
pub async fn reverse_movement(
    db: &Db,
    key: &str,
    user_id: String,
    user_name: String,
    can_reverse: bool,
    body: ReverseCashMovementRequest,
    device_id: Option<String>,
) -> AppResult<CashMovement> {
    tracked("cash_movement.reversed", async move {
        let original = movements::find_cash_movement_by_key(db, key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code(
                    "Cash movement not found",
                    codes::CASH_MOVEMENT_NOT_FOUND,
                )
            })?;
        if original.reverses_key.is_some() || original.movement_type == CashMovementType::NoSale {
            return Err(AppError::validation_with_code(
                "This entry cannot be reversed.",
                codes::MOVEMENT_NOT_REVERSIBLE,
            ));
        }

        // A payment made with a supplier receipt is undone together with what was recorded with
        // it (the owner's own money put in first), so the drawer is never left half-undone.
        let mut members = vec![original.clone()];
        if let Some(receipt_key) = &original.receipt_key {
            for sibling in movements::list_movements_by_receipt_key(db, receipt_key).await? {
                if sibling.key != original.key && sibling.reverses_key.is_none() {
                    members.push(sibling);
                }
            }
        }

        let mut reversals = Vec::with_capacity(members.len());
        let mut original_reversal_key = None;
        for member in &members {
            let is_original = member.key == original.key;
            if !is_original
                && movements::find_cash_movement_by_key(db, &format!("{}_rev", member.key))
                    .await?
                    .is_some()
            {
                continue; // that part was already undone on its own
            }
            let reversal = prepare_reversal(
                db,
                member,
                user_id.clone(),
                user_name.clone(),
                can_reverse,
                body.reason.as_deref(),
                device_id.clone(),
            )
            .await?;
            if is_original {
                original_reversal_key = Some(reversal.key.clone());
            }
            reversals.push(reversal);
        }

        let mut tx = db.begin().await?;
        let mut requested = None;
        for reversal in reversals {
            let is_requested = original_reversal_key.as_deref() == Some(reversal.key.as_str());
            let inserted = write_reversal(&mut tx, reversal).await?;
            if is_requested {
                requested = Some(inserted);
            }
        }
        tx.commit().await?;
        let requested = requested
            .ok_or_else(|| AppError::internal("The reversal that was asked for was not written"))?;
        one_movement(db, requested).await
    })
    .await
}

/// Checks that one movement can be reversed by this person now and builds its opposite.
async fn prepare_reversal(
    db: &Db,
    original: &CashMovementDocument,
    user_id: String,
    user_name: String,
    can_reverse: bool,
    reason: Option<&str>,
    device_id: Option<String>,
) -> AppResult<CashMovementDocument> {
    let reversal_key = format!("{}_rev", original.key);
    if movements::find_cash_movement_by_key(db, &reversal_key)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            codes::MOVEMENT_ALREADY_REVERSED,
            "This movement was already reversed.",
        ));
    }
    if let Some(session_key) = &original.session_key {
        let session = sessions::find_session_by_key(db, session_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code(
                    "Register session not found",
                    codes::SESSION_NOT_FOUND,
                )
            })?;
        if !matches!(
            session.status,
            SessionStatus::Open | SessionStatus::Reopened
        ) {
            return Err(AppError::validation_with_code(
                "That shift is closed. Reopen it first to reverse this movement.",
                codes::NO_ACTIVE_SESSION,
            ));
        }
    }

    let age = chrono::Utc::now() - original.created_at.to_chrono();
    let own_undo = original.recorded_by_user_id == user_id
        && age.num_seconds() <= cash_settings::service::undo_window_seconds(db).await?;
    let reason = clean_text("The reason", reason, NOTE_MAX)?;
    if !own_undo {
        if !can_reverse {
            return Err(AppError::forbidden_with_code(
                "Only a manager can reverse this now.",
                codes::PERMISSION_DENIED,
            ));
        }
        if reason.is_none() {
            return Err(AppError::validation(
                "Please say why this is being reversed.",
            ));
        }
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
    Ok(CashMovementDocument {
        id: None,
        key: reversal_key,
        movement_number,
        movement_type: original.movement_type,
        // The opposite way round, so totals by direction cancel out.
        direction: match original.direction {
            CashDirection::Out => CashDirection::In,
            CashDirection::In => CashDirection::Out,
            CashDirection::Neutral => CashDirection::Neutral,
        },
        amount_cents: original.amount_cents,
        session_key: original.session_key.clone(),
        terminal_id: original.terminal_id.clone(),
        category_key: original.category_key.clone(),
        custom_label: original.custom_label.clone(),
        payee_key: original.payee_key.clone(),
        payee_name: original.payee_name.clone(),
        period_start: None,
        period_end: None,
        reference_no: None,
        note: Some(format!("Reversal of {}", original.movement_number)),
        recorded_by_user_id: user_id,
        recorded_by_name_snapshot: user_name,
        approved_by_user_id: None,
        approved_by_name_snapshot: None,
        approval_reason: reason,
        reverses_key: Some(original.key.clone()),
        recorded_late: false,
        original_variance_session_key: None,
        receipt_key: None,
        version: 1,
        created_at: now,
        updated_at: now,
    })
}

/// Writes a reversal and takes the amount back out of the shift and the drawer.
async fn write_reversal(
    tx: &mut Tx,
    reversal: CashMovementDocument,
) -> AppResult<CashMovementDocument> {
    let session_key = reversal.session_key.clone();
    let terminal_id = reversal.terminal_id.clone();
    let movement_type = reversal.movement_type;
    let amount_cents = reversal.amount_cents;
    let inserted = movements::insert_cash_movement(&mut *tx, reversal).await?;
    if let Some(session_key) = &session_key {
        sessions::increment_session_movements(&mut *tx, session_key, movement_type, -amount_cents)
            .await?;
    }
    let delta = -drawer_delta(movement_type, amount_cents);
    if delta != 0 {
        drawer::update_drawer_balance(&mut *tx, &terminal_id, delta).await?;
    }
    Ok(inserted)
}

/// Which of these movement keys have been reversed.
pub(crate) async fn reversed_keys(
    db: &Db,
    keys: &[String],
) -> AppResult<std::collections::HashSet<String>> {
    Ok(movements::find_reversals_of(db, keys)
        .await?
        .into_iter()
        .map(|(original, _)| original)
        .collect())
}

/// Converts stored rows to the API shape: the live category name, and the reversal of each if one exists.
pub(crate) async fn to_movements(
    db: &Db,
    docs: Vec<CashMovementDocument>,
) -> AppResult<Vec<CashMovement>> {
    // Nothing to convert, nothing to look up (a shop with no movements yet need not have its rules row).
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let names = category_names(db, &docs).await?;
    let undo_window_seconds = cash_settings::service::undo_window_seconds(db).await?;
    let keys: Vec<String> = docs.iter().map(|d| d.key.clone()).collect();
    let reversals: HashMap<String, String> = movements::find_reversals_of(db, &keys)
        .await?
        .into_iter()
        .collect();
    Ok(docs
        .into_iter()
        .map(|doc| {
            let name = names
                .get(&doc.category_key)
                .cloned()
                .expect("category_names resolves the category of every row it is given");
            let reversed_by = reversals.get(&doc.key).cloned();
            doc.into_cash_movement(name, reversed_by, undo_window_seconds)
        })
        .collect())
}

/// Books a validated movement on the shift and the drawer, in the caller's transaction, so the
/// row, the shift's totals and the drawer's cash move together or not at all.
pub(crate) async fn write_movement(
    tx: &mut Tx,
    document: CashMovementDocument,
) -> AppResult<CashMovementDocument> {
    let session_key = document.session_key.clone();
    let terminal_id = document.terminal_id.clone();
    let movement_type = document.movement_type;
    let amount_cents = document.amount_cents;

    let inserted = movements::insert_cash_movement(&mut *tx, document).await?;
    if let Some(session_key) = &session_key {
        sessions::increment_session_movements(&mut *tx, session_key, movement_type, amount_cents)
            .await?;
    }
    // The drawer balance survives across shifts, so it moves with every movement.
    let delta = drawer_delta(movement_type, amount_cents);
    if delta != 0 {
        drawer::update_drawer_balance(&mut *tx, &terminal_id, delta).await?;
    }
    Ok(inserted)
}
