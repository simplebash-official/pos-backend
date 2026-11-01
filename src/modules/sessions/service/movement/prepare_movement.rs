use super::service_movement::NOTE_MAX;
use super::service_movement::cash_in_drawer;
use super::service_movement::clean_text;
use crate::clients::db::Db;

use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::cash_categories::PayeeType;

use crate::domain::recurring_expenses::PeriodStatusQuery;
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::sessions::{CashMovementType, CreateCashMovementRequest};

use crate::modules::sessions::model::CashMovementDocument;

use crate::modules::{
    cash_categories, cash_settings, payees, recurring_expenses, sequences, users,
};
use chrono::NaiveDate;
use mongodb::bson::DateTime as BsonDateTime;

const LABEL_MAX: usize = 60;

const REFERENCE_MAX: usize = 60;

fn parse_period(start: Option<&str>, end: Option<&str>) -> AppResult<Option<(String, String)>> {
    match (start.map(str::trim), end.map(str::trim)) {
        (None, None) => Ok(None),
        (Some(s), Some(e)) => {
            let parse = |v: &str| {
                NaiveDate::parse_from_str(v, "%Y-%m-%d")
                    .map_err(|_| AppError::validation("Please pick a valid period."))
            };
            if parse(s)? > parse(e)? {
                return Err(AppError::validation("The period must end after it starts."));
            }
            Ok(Some((s.to_string(), e.to_string())))
        }
        _ => Err(AppError::validation(
            "Please give both the start and the end of the period.",
        )),
    }
}

/// Refuses a field that does not belong to the movement type, so a stale form cannot save data nothing will read.
fn forbid(applies: bool, present: bool, what: &str) -> AppResult<()> {
    if !applies && present {
        return Err(AppError::validation(format!(
            "{what} does not apply to this kind of payment."
        )));
    }
    Ok(())
}

/// Everything `record_movement` checks, with nothing written: the shift, the amount, the
/// category, the fields this type needs, the payee, the drawer limit and any manager PIN, and
/// the movement number taken. Returns the row ready to insert. `receipt_key` marks a movement
/// recorded together with a supplier receipt; `cash_added_cents` is cash that goes into the
/// drawer earlier in the same save (the owner's own money), counted when checking the drawer limit.
pub(crate) async fn prepare_movement(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateCashMovementRequest,
    device_id: Option<String>,
    receipt_key: Option<String>,
    cash_added_cents: i64,
) -> AppResult<CashMovementDocument> {
    {
        let movement_type = body.movement_type;
        if body.terminal_id.trim().is_empty() {
            return Err(AppError::validation("terminalId is required"));
        }

        let cash_in_drawer = cash_in_drawer(db, body.session_key.as_deref(), &recorded_by_user_id)
            .await?
            + cash_added_cents;

        if movement_type == CashMovementType::NoSale {
            if body.amount_cents != 0 {
                return Err(AppError::validation("A no-sale open has no amount."));
            }
        } else if body.amount_cents <= 0 {
            return Err(AppError::validation(
                "Movement amount must be greater than zero",
            ));
        }

        let category =
            cash_categories::service::category_for_movement(db, &body.category_key, movement_type)
                .await?;
        let custom_label = clean_text("The label", body.custom_label.as_deref(), LABEL_MAX)?;
        if custom_label.is_some()
            && category.key != cash_categories::service::other_category_key(movement_type)
        {
            return Err(AppError::validation(
                "A one-off label can only be used with the Other category.",
            ));
        }
        let reference_no =
            clean_text("The reference", body.reference_no.as_deref(), REFERENCE_MAX)?;
        let note = clean_text("The note", body.note.as_deref(), NOTE_MAX)?;
        let period = parse_period(body.period_start.as_deref(), body.period_end.as_deref())?;
        let typed_payee = clean_text("The name", body.payee_name.as_deref(), 60)?;

        use CashMovementType as T;
        let has_payee = body.payee_key.is_some() || typed_payee.is_some();
        forbid(
            !matches!(movement_type, T::CashDrop | T::NoSale),
            has_payee,
            "A name",
        )?;
        forbid(
            matches!(movement_type, T::RentAndBills | T::StaffPayment),
            period.is_some(),
            "A period",
        )?;
        forbid(
            matches!(
                movement_type,
                T::PettyExpense | T::RentAndBills | T::SupplierPayment | T::AddCash
            ),
            reference_no.is_some(),
            "A reference",
        )?;

        // Who was paid. A saved payee is resolved from the module that owns it, never trusted from the client.
        let mut payee_key = body.payee_key.clone();
        let mut payee_name = typed_payee;
        if let Some(key) = &payee_key {
            let resolved = payees::service::resolve_payee(db, key).await?;
            let expected = match movement_type {
                T::StaffPayment => Some(PayeeType::Staff),
                T::SupplierPayment => Some(PayeeType::Supplier),
                T::RentAndBills => Some(PayeeType::Vendor),
                _ => None,
            };
            if expected.is_some_and(|e| e != resolved.payee_type) {
                return Err(AppError::validation(
                    "This person cannot be paid for this kind of payment.",
                ));
            }
            payee_name = Some(resolved.name);
        }
        match movement_type {
            T::StaffPayment => {
                if payee_key.is_none() {
                    return Err(AppError::validation("Please pick the staff member."));
                }
            }
            T::SupplierPayment => {
                if payee_key.is_none() {
                    return Err(AppError::validation("Please pick the supplier."));
                }
            }
            T::RentAndBills => {
                if payee_key.is_none() {
                    return Err(AppError::validation("Please pick who is being paid."));
                }
                if period.is_none() {
                    return Err(AppError::validation(
                        "Please pick the period this payment covers.",
                    ));
                }
            }
            T::OwnerWithdrawal => {
                if payee_name.is_none() {
                    return Err(AppError::validation("Please enter who took the cash."));
                }
            }
            T::PettyExpense | T::AddCash => {
                // A name typed once is saved so it is offered next time.
                if payee_key.is_none()
                    && let Some(name) = &payee_name
                {
                    let saved = payees::service::ensure_other_payee(db, name).await?;
                    payee_key = Some(saved.key);
                    payee_name = Some(saved.name);
                }
            }
            T::CashDrop | T::NoSale => {}
        }

        // Rent and bills for a month that is already paid is asked about, not silently doubled.
        if movement_type == T::RentAndBills
            && !body.confirm_duplicate
            && let (Some(payee), Some((start, _))) = (&payee_key, &period)
        {
            let status = recurring_expenses::service::period_status(
                db,
                PeriodStatusQuery {
                    category_key: category.key.clone(),
                    payee_key: Some(payee.clone()),
                    period: start[..7].to_string(),
                },
            )
            .await?;
            if status.already_paid {
                let last = &status.payments[status.payments.len() - 1];
                return Err(AppError::conflict(
                    codes::PERIOD_ALREADY_PAID,
                    format!(
                        "{} for {} already has {} recorded (last on {}). Confirm if this is another payment.",
                        category.name,
                        start.get(..7).unwrap_or(start),
                        money(status.paid_cents),
                        last.date
                    ),
                ));
            }
        }

        // A payout bigger than the cash in the drawer is allowed only when a manager says why.
        let takes_cash_out = movement_type.takes_cash_out();
        let over_drawer = movement_type.is_over_drawer(body.amount_cents, cash_in_drawer);
        let override_reason = clean_text("The reason", body.override_reason.as_deref(), NOTE_MAX)?;
        if over_drawer {
            if body.manager_pin.is_none() || override_reason.is_none() {
                return Err(AppError::conflict(
                    codes::DRAWER_CASH_EXCEEDED,
                    format!(
                        "The drawer only holds {}. To pay out more, a manager or owner needs to enter their PIN and say why.",
                        money(cash_in_drawer)
                    ),
                ));
            }
        } else {
            // One approval can cover several payments of a supplier receipt, so its reason may
            // ride along on a payment that does not go over the drawer; it is not stored there.
            forbid(
                receipt_key.is_some(),
                override_reason.is_some(),
                "A reason for going over the drawer",
            )?;
        }

        // A manager PIN: always for owner drawings and staff pay, above the category's own limit or the
        // shop-wide one, and to go over the drawer.
        // The shop-wide limit is about cash leaving the drawer; cash put in never needs it.
        let shop_limit = if takes_cash_out {
            cash_settings::service::approval_threshold_cents(db).await?
        } else {
            None
        };
        let pin_needed = movement_type.needs_manager_pin(
            body.amount_cents,
            category.requires_approval_over_cents,
            shop_limit,
            over_drawer,
        );
        let mut approved_by_id = None;
        let mut approved_by_name = None;
        if pin_needed {
            let pin = body.manager_pin.as_deref().ok_or_else(|| {
                AppError::forbidden_with_code(
                    "A manager or owner PIN is required for this payment",
                    codes::PIN_NOT_SET,
                )
            })?;
            let approver = users::service::verify_user_pin(db, pin, Some("manager"))
                .await?
                .ok_or_else(|| {
                    AppError::unauthorized_with_code(
                        "Invalid manager/owner PIN",
                        codes::INVALID_PIN,
                    )
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
        Ok(CashMovementDocument {
            id: None,
            key: generate_id(prefixes::CASH_MOVEMENT),
            movement_number,
            movement_type,
            direction: movement_type.direction(),
            amount_cents: body.amount_cents,
            session_key: body.session_key.clone(),
            terminal_id: body.terminal_id.clone(),
            category_key: category.key.clone(),
            custom_label,
            payee_key,
            payee_name,
            period_start: period.as_ref().map(|p| p.0.clone()),
            period_end: period.map(|p| p.1),
            reference_no,
            note,
            recorded_by_user_id,
            recorded_by_name_snapshot: recorded_by_name,
            approved_by_user_id: approved_by_id,
            approved_by_name_snapshot: approved_by_name,
            approval_reason: if over_drawer { override_reason } else { None },
            reverses_key: None,
            recorded_late: false,
            original_variance_session_key: None,
            receipt_key,
            version: 1,
            created_at: now,
            updated_at: now,
        })
    }
}

/// `Rs. 1,234.50`, for messages that tell a person how much is in the drawer.
fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let whole = (cents.abs() / 100).to_string();
    let mut grouped = String::new();
    for (i, digit) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{sign}Rs. {grouped}.{:02}", cents.abs() % 100)
}
