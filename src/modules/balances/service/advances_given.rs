use super::given::Given;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::cash_categories::category_key;
use crate::domain::sessions::CashMovementType;
use crate::modules::{cash_categories, expenses, sessions};

/// The staff-payment categories that are money the staff member owes back.
const ADVANCE_CATEGORIES: [&str; 2] = ["Advance", "Staff loan"];

/// Every advance or staff loan handed out, to anyone with a staff key.
pub(super) async fn advances_given(db: &Db) -> AppResult<Vec<Given>> {
    let mut keys = Vec::new();
    for name in ADVANCE_CATEGORIES {
        keys.extend(
            cash_categories::service::keys_including_merged(
                db,
                &category_key(CashMovementType::StaffPayment, name),
            )
            .await?,
        );
    }
    let is_advance = |category: &str| keys.iter().any(|k| k == category);
    let is_staff = |payee: Option<&str>| payee.is_some_and(|k| k.starts_with("emp_"));
    let mut out: Vec<Given> =
        sessions::service::payouts_of_type(db, CashMovementType::StaffPayment)
            .await?
            .into_iter()
            .filter(|p| is_advance(&p.category_key) && is_staff(p.payee_key.as_deref()))
            .map(|p| Given {
                employee_key: p.payee_key.expect("filtered to staff"),
                category_key: p.category_key,
                day: p.day,
                number: p.number,
                cents: p.amount_cents,
            })
            .collect();
    out.extend(
        expenses::service::ledger_payouts(db)
            .await?
            .into_iter()
            .filter(|p| is_advance(&p.category_key) && is_staff(p.payee_key.as_deref()))
            .map(|p| Given {
                employee_key: p.payee_key.expect("filtered to staff"),
                category_key: p.category_key,
                day: p.day,
                number: p.number,
                cents: p.amount_cents,
            }),
    );
    Ok(out)
}
