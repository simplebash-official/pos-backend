//! The warranty desk's headline numbers and its late claims, for the Warranties page, the
//! dashboard and the dashboard's attention list.

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::utils::{shop_day, shop_midnight};
use crate::domain::reports::{ReminderEntry, ReminderKind};
use crate::domain::warranties::{CLAIM_SLOW_AFTER_DAYS, ClaimStatus, WarrantyStats};
use crate::modules::repairs;
use crate::modules::warranties::model::{WarrantyClaimDocument, stored_day};
use crate::modules::warranties::repository::{self, ClaimFilter};
use chrono::{Datelike, Duration, Months, NaiveDate, Utc};
use pos_calc_core::warranty;

/// Every status a claim can be in before it is resolved.
const OPEN_STATUSES: [ClaimStatus; 4] = [
    ClaimStatus::Received,
    ClaimStatus::Inspected,
    ClaimStatus::Decided,
    ClaimStatus::AtSupplier,
];

/// A late claim waiting this many days past its due day is marked critical.
const CRITICAL_AFTER_DAYS: i64 = 7;

/// Why an open claim is late, and since when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lateness {
    /// With the supplier after the day they said it would be back (that day).
    SupplierLate { due_on: NaiveDate },
    /// Open longer than a week (the day it became too long).
    Slow { due_on: NaiveDate },
}

impl Lateness {
    fn due_on(self) -> NaiveDate {
        match self {
            Lateness::SupplierLate { due_on } | Lateness::Slow { due_on } => due_on,
        }
    }
}

/// Whether an open claim is late on `today`: a supplier past the day they gave comes first, then
/// a claim open more than `CLAIM_SLOW_AFTER_DAYS` days. The same rule the claims queue marks in
/// orange.
pub(crate) fn lateness(
    claim: &WarrantyClaimDocument,
    today: NaiveDate,
) -> AppResult<Option<Lateness>> {
    if !claim.status.is_open() {
        return Ok(None);
    }
    if claim.status == ClaimStatus::AtSupplier
        && let Some(supplier) = &claim.supplier
        && supplier.received_back_on.is_none()
        && let Some(expected) = &supplier.expected_back_on
    {
        let due_on = stored_day("expected_back_on", expected)?;
        if due_on < today {
            return Ok(Some(Lateness::SupplierLate { due_on }));
        }
    }
    let slow_from =
        stored_day("received_on", &claim.received_on)? + Duration::days(CLAIM_SLOW_AFTER_DAYS);
    Ok((slow_from < today).then_some(Lateness::Slow { due_on: slow_from }))
}

async fn open_claims(db: &Db) -> AppResult<Vec<WarrantyClaimDocument>> {
    repository::all_claims(
        db,
        &ClaimFilter {
            statuses: Some(&OPEN_STATUSES),
            ..ClaimFilter::default()
        },
    )
    .await
}

/// The warranty desk at a glance, for the shop-local today.
pub(crate) async fn stats(db: &Db) -> AppResult<WarrantyStats> {
    let today = shop_day(Utc::now());
    let until = warranty::expiring_soon_until(today);
    let counts = repository::count_in_force(
        db,
        &today.format("%Y-%m-%d").to_string(),
        &until.format("%Y-%m-%d").to_string(),
    )
    .await?;

    let open = open_claims(db).await?;
    let mut late_claim_count = 0;
    for claim in &open {
        if lateness(claim, today)?.is_some() {
            late_claim_count += 1;
        }
    }

    let month_start = today.with_day(1).expect("every month has a first day");
    let next_month = month_start
        .checked_add_months(Months::new(1))
        .expect("next month is a valid date");
    let rework =
        repairs::service::rework_costs(db, shop_midnight(month_start), shop_midnight(next_month))
            .await?;

    Ok(WarrantyStats {
        in_force_count: counts.in_force,
        ending_soon_count: counts.ending_soon,
        ending_soon_until: until.format("%Y-%m-%d").to_string(),
        open_claim_count: u64::try_from(open.len()).expect("a claim count fits in u64"),
        late_claim_count,
        rework_cost_this_month_cents: rework.iter().map(|job| job.cost_cents).sum(),
    })
}

/// One attention-list row per late claim: overdue by the days past its due day, critical after
/// another week.
pub(crate) async fn late_claim_reminders(db: &Db) -> AppResult<Vec<ReminderEntry>> {
    let today = shop_day(Utc::now());
    let mut reminders = Vec::new();
    for claim in open_claims(db).await? {
        let Some(late) = lateness(&claim, today)? else {
            continue;
        };
        let days_from_due = (today - late.due_on()).num_days();
        reminders.push(ReminderEntry {
            kind: match late {
                Lateness::SupplierLate { .. } => ReminderKind::ClaimSupplierLate,
                Lateness::Slow { .. } => ReminderKind::ClaimSlow,
            },
            id: claim
                .id
                .ok_or_else(|| AppError::internal("persisted claim must have an id"))?
                .to_hex(),
            key: claim.key.clone(),
            reference_number: claim.claim_number,
            title: claim.item_name,
            customer_name: claim.customer_name,
            customer_phone: claim.customer_phone,
            amount_cents: 0,
            due_date: late.due_on().format("%Y-%m-%d").to_string(),
            days_from_due,
            severity: if days_from_due > CRITICAL_AFTER_DAYS {
                "critical"
            } else {
                "warning"
            }
            .to_string(),
            link_to: format!("/warranties?view=claims&claim={}", claim.key),
        });
    }
    Ok(reminders)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::warranties::{SupplierClaim, VoidChecks};
    use mongodb::bson::DateTime as BsonDateTime;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("test day")
    }

    fn claim(status: ClaimStatus, received_on: &str) -> WarrantyClaimDocument {
        WarrantyClaimDocument {
            id: None,
            key: "wcl_1".to_string(),
            claim_number: "WCL-000001".to_string(),
            warranty_key: "wty_1".to_string(),
            warranty_number: "WTY-000001".to_string(),
            item_name: "Phone".to_string(),
            serial_number: None,
            product_key: None,
            invoice_key: "inv_1".to_string(),
            invoice_number: "INV-000001".to_string(),
            customer_key: None,
            customer_name: None,
            customer_phone: None,
            issue_description: "No power".to_string(),
            void_checks: VoidChecks {
                screen_cracked: false,
                liquid_damage: false,
                seal_broken: false,
                tampered: false,
            },
            assigned_employee_key: None,
            assigned_employee_name: None,
            status,
            inspection_notes: None,
            decision: None,
            decision_reason: None,
            decided_by_user_id: None,
            decided_by_name: None,
            expired_override_by_name: None,
            resolution_note: None,
            replacement_serial_number: None,
            replacement_warranty_key: None,
            credit_note_key: None,
            paid_repair_key: None,
            paid_repair_number: None,
            rework_repair_key: None,
            rework_repair_number: None,
            supplier: None,
            received_on: received_on.to_string(),
            resolved_on: None,
            returned_on: None,
            version: 1,
            created_at: BsonDateTime::now(),
            updated_at: BsonDateTime::now(),
            updated_by_device: None,
        }
    }

    fn at_supplier(received_on: &str, expected: Option<&str>) -> WarrantyClaimDocument {
        let mut claim = claim(ClaimStatus::AtSupplier, received_on);
        claim.supplier = Some(SupplierClaim {
            supplier_key: "sup_1".to_string(),
            supplier_name: "Acme".to_string(),
            sent_on: received_on.to_string(),
            reference: None,
            expected_back_on: expected.map(str::to_string),
            received_back_on: None,
            outcome: None,
            note: None,
        });
        claim
    }

    #[test]
    fn a_claim_is_slow_after_a_full_week() {
        let today = day("2026-11-10");
        assert_eq!(
            lateness(&claim(ClaimStatus::Received, "2026-11-03"), today).expect("readable"),
            None,
            "seven days is still on time"
        );
        assert_eq!(
            lateness(&claim(ClaimStatus::Inspected, "2026-11-02"), today).expect("readable"),
            Some(Lateness::Slow {
                due_on: day("2026-11-09")
            })
        );
    }

    #[test]
    fn a_closed_claim_is_never_late() {
        let today = day("2026-12-31");
        for status in [ClaimStatus::Resolved, ClaimStatus::Returned] {
            assert_eq!(
                lateness(&claim(status, "2026-01-01"), today).expect("readable"),
                None
            );
        }
    }

    #[test]
    fn a_supplier_past_its_day_comes_first() {
        let today = day("2026-11-10");
        assert_eq!(
            lateness(&at_supplier("2026-11-01", Some("2026-11-08")), today).expect("readable"),
            Some(Lateness::SupplierLate {
                due_on: day("2026-11-08")
            })
        );
        assert_eq!(
            lateness(&at_supplier("2026-11-08", Some("2026-11-10")), today).expect("readable"),
            None,
            "due back today is not late yet"
        );
        assert_eq!(
            lateness(&at_supplier("2026-10-01", None), today).expect("readable"),
            Some(Lateness::Slow {
                due_on: day("2026-10-08")
            }),
            "no day given: only the week rule applies"
        );
    }
}
