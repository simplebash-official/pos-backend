//! The warranty report: how often warranties given in a period were claimed, by product, by
//! repair service and by technician; what suppliers did with the claims sent to them; and what
//! free rework cost each month.

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::utils::shop_day;
use crate::domain::warranties::{
    ClaimRateRow, ReworkMonth, SupplierClaimRow, SupplierOutcome, WarrantyReport,
    WarrantySourceType, WarrantyStatus,
};
use crate::modules::repairs;
use crate::modules::repairs::service::{JobTechnician, ReworkCost};
use crate::modules::warranties::model::{WarrantyClaimDocument, WarrantyDocument, stored_day};
use crate::modules::warranties::repository;
use chrono::{DateTime, NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;
use std::collections::{BTreeMap, HashMap};

/// Basis points in a whole.
const WHOLE_BPS: i64 = 10_000;

/// A warranty given in the period that tells something about quality. One cancelled without a
/// claim was a sale undone (voided or returned), and one that replaces another only carries on
/// that unit's cover, so neither counts.
fn counts_as_issued(warranty: &WarrantyDocument) -> bool {
    let undone = warranty.status == WarrantyStatus::Void && warranty.claim_count == 0;
    !undone && warranty.replaces_warranty_key.is_none()
}

/// `part / whole` in basis points, rounded half up; zero of nothing is zero.
#[must_use]
fn rate_bps(part: u64, whole: u64) -> i64 {
    if whole == 0 {
        return 0;
    }
    let part = i64::try_from(part).expect("a warranty count fits in i64");
    let whole = i64::try_from(whole).expect("a warranty count fits in i64");
    (part * WHOLE_BPS + whole / 2) / whole
}

/// One group's running counts, named after its newest warranty.
#[derive(Debug)]
struct Tally {
    name: Option<String>,
    newest: BsonDateTime,
    issued: u64,
    claimed: u64,
}

fn add<K: Ord>(groups: &mut BTreeMap<K, Tally>, key: K, name: Option<&str>, w: &WarrantyDocument) {
    let tally = groups.entry(key).or_insert_with(|| Tally {
        name: name.map(str::to_string),
        newest: w.created_at,
        issued: 0,
        claimed: 0,
    });
    if w.created_at > tally.newest {
        tally.newest = w.created_at;
        tally.name = name.map(str::to_string);
    }
    tally.issued += 1;
    if w.claim_count > 0 {
        tally.claimed += 1;
    }
}

/// Most claimed first, then the highest rate, then the most given, then by name.
fn into_rows<K>(groups: BTreeMap<K, Tally>) -> Vec<ClaimRateRow> {
    let mut rows: Vec<ClaimRateRow> = groups
        .into_values()
        .map(|tally| ClaimRateRow {
            name: tally.name,
            issued_count: tally.issued,
            claimed_count: tally.claimed,
            claim_rate_bps: rate_bps(tally.claimed, tally.issued),
        })
        .collect();
    rows.sort_by(|a, b| {
        b.claimed_count
            .cmp(&a.claimed_count)
            .then_with(|| b.claim_rate_bps.cmp(&a.claim_rate_bps))
            .then_with(|| b.issued_count.cmp(&a.issued_count))
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

/// The three claim-rate tables. Every repair warranty's job must be in `technicians`.
struct ClaimRates {
    by_product: Vec<ClaimRateRow>,
    by_repair_service: Vec<ClaimRateRow>,
    by_technician: Vec<ClaimRateRow>,
}

fn claim_rates(
    issued: &[WarrantyDocument],
    technicians: &HashMap<String, Option<JobTechnician>>,
) -> AppResult<ClaimRates> {
    let mut products = BTreeMap::new();
    let mut services = BTreeMap::new();
    let mut people = BTreeMap::new();
    for w in issued {
        match w.source_type {
            WarrantySourceType::ProductSale => {
                let key = match &w.product_key {
                    Some(product) => product.clone(),
                    None => format!("name:{}", w.item_name),
                };
                add(&mut products, key, Some(&w.item_name), w);
            }
            WarrantySourceType::Repair => {
                let service = w.item_name.trim().to_lowercase();
                add(&mut services, service, Some(&w.item_name), w);
                let job = w.repair_key.as_ref().ok_or_else(|| {
                    AppError::internal(format!("repair warranty `{}` has no job", w.key))
                })?;
                let technician = technicians.get(job).ok_or_else(|| {
                    AppError::internal(format!("repair warranty `{}` names a missing job", w.key))
                })?;
                match technician {
                    Some(person) => {
                        add(
                            &mut people,
                            Some(person.employee_id.clone()),
                            Some(&person.name),
                            w,
                        );
                    }
                    None => add(&mut people, None, None, w),
                }
            }
        }
    }
    Ok(ClaimRates {
        by_product: into_rows(products),
        by_repair_service: into_rows(services),
        by_technician: into_rows(people),
    })
}

/// What each supplier did with the claims sent to them, most sent first.
fn supplier_rows(
    sent: &[WarrantyClaimDocument],
    today: NaiveDate,
) -> AppResult<Vec<SupplierClaimRow>> {
    struct Trips {
        row: SupplierClaimRow,
        latest_sent: String,
        days_away: Vec<i64>,
    }
    let mut suppliers: BTreeMap<String, Trips> = BTreeMap::new();
    for claim in sent {
        let trip = claim.supplier.as_ref().ok_or_else(|| {
            AppError::internal(format!(
                "claim `{}` was listed as sent but has no supplier",
                claim.key
            ))
        })?;
        let entry = suppliers
            .entry(trip.supplier_key.clone())
            .or_insert_with(|| Trips {
                row: SupplierClaimRow {
                    supplier_key: trip.supplier_key.clone(),
                    supplier_name: trip.supplier_name.clone(),
                    sent_count: 0,
                    away_count: 0,
                    overdue_count: 0,
                    repaired_count: 0,
                    replaced_count: 0,
                    credited_count: 0,
                    rejected_count: 0,
                    average_days_away: None,
                },
                latest_sent: trip.sent_on.clone(),
                days_away: Vec::new(),
            });
        if trip.sent_on > entry.latest_sent {
            entry.latest_sent = trip.sent_on.clone();
            entry.row.supplier_name = trip.supplier_name.clone();
        }
        entry.row.sent_count += 1;
        match &trip.received_back_on {
            None => {
                entry.row.away_count += 1;
                if let Some(expected) = &trip.expected_back_on
                    && stored_day("expected_back_on", expected)? < today
                {
                    entry.row.overdue_count += 1;
                }
            }
            Some(back) => {
                let days = (stored_day("received_back_on", back)?
                    - stored_day("sent_on", &trip.sent_on)?)
                .num_days();
                entry.days_away.push(days);
            }
        }
        match trip.outcome {
            Some(SupplierOutcome::Repaired) => entry.row.repaired_count += 1,
            Some(SupplierOutcome::Replaced) => entry.row.replaced_count += 1,
            Some(SupplierOutcome::Credited) => entry.row.credited_count += 1,
            Some(SupplierOutcome::Rejected) => entry.row.rejected_count += 1,
            None => {}
        }
    }
    let mut rows: Vec<SupplierClaimRow> = suppliers
        .into_values()
        .map(|mut trips| {
            if !trips.days_away.is_empty() {
                let count = i64::try_from(trips.days_away.len()).expect("a trip count fits");
                let total: i64 = trips.days_away.iter().sum();
                trips.row.average_days_away = Some((total + count / 2) / count);
            }
            trips.row
        })
        .collect();
    rows.sort_by(|a, b| {
        b.sent_count
            .cmp(&a.sent_count)
            .then_with(|| a.supplier_name.cmp(&b.supplier_name))
    });
    Ok(rows)
}

/// Rework jobs and their cost by shop-local month, oldest first.
fn rework_months(costs: &[ReworkCost]) -> Vec<ReworkMonth> {
    let mut months: BTreeMap<String, ReworkMonth> = BTreeMap::new();
    for cost in costs {
        let month = cost.opened_on.format("%Y-%m").to_string();
        let entry = months.entry(month.clone()).or_insert_with(|| ReworkMonth {
            month,
            job_count: 0,
            cost_cents: 0,
        });
        entry.job_count += 1;
        entry.cost_cents += cost.cost_cents;
    }
    months.into_values().collect()
}

/// The warranty report for warranties given, and claims sent to suppliers, in `[start, end)`.
pub(crate) async fn report(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<WarrantyReport> {
    let today = shop_day(Utc::now());
    let issued: Vec<WarrantyDocument> = repository::issued_between(db, start, end)
        .await?
        .into_iter()
        .filter(counts_as_issued)
        .collect();
    let mut jobs: Vec<String> = issued.iter().filter_map(|w| w.repair_key.clone()).collect();
    jobs.sort();
    jobs.dedup();
    let technicians = repairs::service::technicians_by_job(db, &jobs).await?;
    let rates = claim_rates(&issued, &technicians)?;

    let sent = repository::sent_to_supplier_between(
        db,
        &shop_day(start).format("%Y-%m-%d").to_string(),
        &shop_day(end).format("%Y-%m-%d").to_string(),
    )
    .await?;
    let rework = repairs::service::rework_costs(db, start, end).await?;

    let issued_count = u64::try_from(issued.len()).expect("a warranty count fits in u64");
    let claimed_count = u64::try_from(issued.iter().filter(|w| w.claim_count > 0).count())
        .expect("a warranty count fits in u64");
    Ok(WarrantyReport {
        period_start: start,
        period_end: end,
        issued_count,
        claimed_count,
        claim_rate_bps: rate_bps(claimed_count, issued_count),
        by_product: rates.by_product,
        by_repair_service: rates.by_repair_service,
        by_technician: rates.by_technician,
        by_supplier: supplier_rows(&sent, today)?,
        rework_by_month: rework_months(&rework),
        rework_job_count: u64::try_from(rework.len()).expect("a job count fits in u64"),
        rework_cost_cents: rework.iter().map(|job| job.cost_cents).sum(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::warranties::{
        ClaimStatus, SupplierClaim, VoidChecks, WarrantyCoverage, WarrantyType,
    };

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("test day")
    }

    fn at(millis: i64) -> BsonDateTime {
        BsonDateTime::from_millis(millis)
    }

    fn warranty(key: &str, source: WarrantySourceType, name: &str) -> WarrantyDocument {
        WarrantyDocument {
            id: None,
            key: key.to_string(),
            warranty_number: format!("WTY-{key}"),
            source_type: source,
            invoice_key: "inv_1".to_string(),
            invoice_number: "INV-000001".to_string(),
            line_index: 0,
            product_key: None,
            repair_key: None,
            repair_item_key: None,
            item_name: name.to_string(),
            serial_number: None,
            quantity: 1,
            claimed_quantity: Some(0),
            customer_key: None,
            customer_name: None,
            customer_phone: None,
            customer_phone_tail: None,
            coverage: WarrantyCoverage::Lifetime,
            warranty_type: WarrantyType::Shop,
            terms: None,
            unit_price_cents: 1_000,
            starts_on: "2026-11-01".to_string(),
            ends_on: None,
            status: WarrantyStatus::Active,
            void_reason: None,
            replaces_warranty_key: None,
            replaced_by_warranty_key: None,
            claim_count: 0,
            version: 1,
            created_at: at(1_000),
            updated_at: at(1_000),
            updated_by_device: None,
        }
    }

    fn sold(key: &str, product: &str, name: &str, claims: i64) -> WarrantyDocument {
        let mut w = warranty(key, WarrantySourceType::ProductSale, name);
        w.product_key = Some(product.to_string());
        w.claim_count = claims;
        w
    }

    fn repaired(key: &str, job: &str, name: &str, claims: i64) -> WarrantyDocument {
        let mut w = warranty(key, WarrantySourceType::Repair, name);
        w.repair_key = Some(job.to_string());
        w.claim_count = claims;
        w
    }

    #[test]
    fn undone_sales_and_replacements_are_not_counted() {
        let mut voided = sold("a", "prod_1", "Phone", 0);
        voided.status = WarrantyStatus::Void;
        let mut claimed_then_voided = sold("b", "prod_1", "Phone", 1);
        claimed_then_voided.status = WarrantyStatus::Void;
        let mut replacement = sold("c", "prod_1", "Phone", 0);
        replacement.replaces_warranty_key = Some("wty_b".to_string());
        assert!(!counts_as_issued(&voided));
        assert!(counts_as_issued(&claimed_then_voided));
        assert!(!counts_as_issued(&replacement));
        assert!(counts_as_issued(&sold("d", "prod_1", "Phone", 0)));
    }

    #[test]
    fn rates_round_half_up_and_nothing_is_zero() {
        assert_eq!(rate_bps(0, 0), 0);
        assert_eq!(rate_bps(1, 3), 3_333);
        assert_eq!(rate_bps(2, 3), 6_667);
        assert_eq!(rate_bps(3, 3), 10_000);
    }

    #[test]
    fn claim_rates_group_by_product_service_and_technician() {
        let mut renamed = sold("p3", "prod_1", "Galaxy A55 5G", 0);
        renamed.created_at = at(9_000);
        let issued = vec![
            sold("p1", "prod_1", "Galaxy A55", 1),
            sold("p2", "prod_1", "Galaxy A55", 0),
            renamed,
            sold("p4", "prod_2", "Charger", 2),
            repaired("r1", "rep_1", "Screen replacement", 1),
            repaired("r2", "rep_2", " screen REPLACEMENT", 0),
            repaired("r3", "rep_3", "Battery", 0),
        ];
        let technicians = HashMap::from([
            (
                "rep_1".to_string(),
                Some(JobTechnician {
                    employee_id: "emp_1".to_string(),
                    name: "Nimal".to_string(),
                }),
            ),
            (
                "rep_2".to_string(),
                Some(JobTechnician {
                    employee_id: "emp_1".to_string(),
                    name: "Nimal".to_string(),
                }),
            ),
            ("rep_3".to_string(), None),
        ]);
        let rates = claim_rates(&issued, &technicians).expect("every job is known");

        assert_eq!(
            rates.by_product,
            vec![
                ClaimRateRow {
                    name: Some("Charger".to_string()),
                    issued_count: 1,
                    claimed_count: 1,
                    claim_rate_bps: 10_000,
                },
                ClaimRateRow {
                    name: Some("Galaxy A55 5G".to_string()),
                    issued_count: 3,
                    claimed_count: 1,
                    claim_rate_bps: 3_333,
                },
            ],
            "a product is named after its newest warranty; a second claim on one unit is still one"
        );
        assert_eq!(
            rates.by_repair_service.len(),
            2,
            "names group regardless of case"
        );
        assert_eq!(rates.by_repair_service[0].issued_count, 2);
        assert_eq!(rates.by_repair_service[0].claimed_count, 1);
        assert_eq!(
            rates.by_technician,
            vec![
                ClaimRateRow {
                    name: Some("Nimal".to_string()),
                    issued_count: 2,
                    claimed_count: 1,
                    claim_rate_bps: 5_000,
                },
                ClaimRateRow {
                    name: None,
                    issued_count: 1,
                    claimed_count: 0,
                    claim_rate_bps: 0,
                },
            ]
        );
    }

    #[test]
    fn a_job_nobody_was_assigned_to_has_no_name_on_the_wire() {
        let row = ClaimRateRow {
            name: None,
            issued_count: 1,
            claimed_count: 0,
            claim_rate_bps: 0,
        };
        assert_eq!(
            serde_json::to_value(&row).expect("a row serializes"),
            serde_json::json!({ "issuedCount": 1, "claimedCount": 0, "claimRateBps": 0 })
        );
    }

    #[test]
    fn a_repair_warranty_without_its_job_is_an_error() {
        let issued = vec![repaired("r1", "rep_gone", "Screen", 0)];
        assert!(claim_rates(&issued, &HashMap::new()).is_err());
    }

    fn sent(
        key: &str,
        supplier: &str,
        sent_on: &str,
        expected: Option<&str>,
        back: Option<(&str, SupplierOutcome)>,
    ) -> WarrantyClaimDocument {
        WarrantyClaimDocument {
            id: None,
            key: key.to_string(),
            claim_number: format!("WCL-{key}"),
            warranty_key: "wty_1".to_string(),
            warranty_number: "WTY-000001".to_string(),
            item_name: "Phone".to_string(),
            serial_number: None,
            quantity: Some(1),
            product_key: None,
            invoice_key: "inv_1".to_string(),
            invoice_number: "INV-000001".to_string(),
            customer_key: None,
            customer_name: None,
            customer_phone: None,
            issue_description: "Dead".to_string(),
            void_checks: VoidChecks {
                physical_damage: false,
                liquid_damage: false,
                seal_broken: false,
                tampered: false,
                screen_cracked: None,
            },
            conditions: Vec::new(),
            assigned_employee_key: None,
            assigned_employee_name: None,
            status: ClaimStatus::AtSupplier,
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
            supplier: Some(SupplierClaim {
                supplier_key: supplier.to_string(),
                supplier_name: format!("{supplier} on {sent_on}"),
                sent_on: sent_on.to_string(),
                reference: None,
                expected_back_on: expected.map(str::to_string),
                received_back_on: back.map(|(on, _)| on.to_string()),
                outcome: back.map(|(_, outcome)| outcome),
                note: None,
            }),
            received_on: sent_on.to_string(),
            resolved_on: None,
            returned_on: None,
            version: 1,
            created_at: at(1_000),
            updated_at: at(1_000),
            updated_by_device: None,
        }
    }

    #[test]
    fn suppliers_count_trips_outcomes_and_lateness() {
        let claims = vec![
            sent(
                "1",
                "sup_a",
                "2026-11-01",
                None,
                Some(("2026-11-04", SupplierOutcome::Repaired)),
            ),
            sent(
                "2",
                "sup_a",
                "2026-11-02",
                None,
                Some(("2026-11-06", SupplierOutcome::Replaced)),
            ),
            sent("3", "sup_a", "2026-11-05", Some("2026-11-08"), None),
            sent("4", "sup_b", "2026-11-06", Some("2026-11-20"), None),
        ];
        let rows = supplier_rows(&claims, day("2026-11-10")).expect("readable days");
        assert_eq!(rows.len(), 2);
        let a = &rows[0];
        assert_eq!(a.supplier_key, "sup_a");
        assert_eq!(
            a.supplier_name, "sup_a on 2026-11-05",
            "named as on the latest trip"
        );
        assert_eq!((a.sent_count, a.away_count, a.overdue_count), (3, 1, 1));
        assert_eq!((a.repaired_count, a.replaced_count), (1, 1));
        assert_eq!(
            a.average_days_away,
            Some(4),
            "3 and 4 days average 3.5, rounded up"
        );
        let b = &rows[1];
        assert_eq!((b.sent_count, b.away_count, b.overdue_count), (1, 1, 0));
        assert_eq!(b.average_days_away, None);
    }

    #[test]
    fn rework_is_summed_by_month() {
        let costs = vec![
            ReworkCost {
                opened_on: day("2026-11-03"),
                cost_cents: 500,
            },
            ReworkCost {
                opened_on: day("2026-10-31"),
                cost_cents: 200,
            },
            ReworkCost {
                opened_on: day("2026-11-30"),
                cost_cents: 0,
            },
        ];
        assert_eq!(
            rework_months(&costs),
            vec![
                ReworkMonth {
                    month: "2026-10".to_string(),
                    job_count: 1,
                    cost_cents: 200,
                },
                ReworkMonth {
                    month: "2026-11".to_string(),
                    job_count: 2,
                    cost_cents: 500,
                },
            ]
        );
    }
}
