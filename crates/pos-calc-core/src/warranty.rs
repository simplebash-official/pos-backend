//! Warranty periods: what a coverage is, the last day it covers, and how much of it is left.
//! Dates are shop-local calendar dates; the caller passes "today" so the browser and the
//! server agree on the same day.

use chrono::{Datelike, Days, Months, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::error::{CalcError, CalcResult};

/// A warranty this close to its end (in days, the last day counting as one) is "expiring soon".
pub const EXPIRING_SOON_DAYS: i64 = 7;

/// The unit a limited warranty is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum WarrantyUnit {
    Days,
    Weeks,
    Months,
    Years,
}

impl WarrantyUnit {
    /// The largest count allowed in this unit: about a hundred years.
    #[must_use]
    pub fn max_value(self) -> u32 {
        match self {
            WarrantyUnit::Days => 36_500,
            WarrantyUnit::Weeks => 5_200,
            WarrantyUnit::Months => 1_200,
            WarrantyUnit::Years => 100,
        }
    }

    /// The name this unit has on the wire and in storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WarrantyUnit::Days => "days",
            WarrantyUnit::Weeks => "weeks",
            WarrantyUnit::Months => "months",
            WarrantyUnit::Years => "years",
        }
    }
}

/// How long something is covered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum WarrantyCoverage {
    /// Not covered at all.
    None,
    /// Covered for `value` `unit`s from the start day.
    Limited { value: u32, unit: WarrantyUnit },
    /// Covered with no end date.
    Lifetime,
}

impl WarrantyCoverage {
    /// Checks a limited period is at least one unit and at most about a hundred years.
    ///
    /// # Errors
    ///
    /// Fails with a message fit to show the user when the count is out of range.
    pub fn validate(&self) -> CalcResult<()> {
        if let WarrantyCoverage::Limited { value, unit } = *self {
            if value == 0 {
                return Err(CalcError::validation(
                    "A warranty period must be at least 1. Choose \"No warranty\" instead.",
                ));
            }
            if value > unit.max_value() {
                return Err(CalcError::validation(format!(
                    "A warranty can be at most {} {}.",
                    unit.max_value(),
                    unit.as_str()
                )));
            }
        }
        Ok(())
    }
}

/// When a warranty that starts on a given day stops covering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarrantyEnd {
    /// The last day it covers, inclusive.
    On(NaiveDate),
    /// It never ends.
    Never,
}

/// The last day a warranty starting on `starts_on` covers.
///
/// A period ends the day before the same date one period later: 12 months from
/// 3 Nov 2026 covers up to and including 2 Nov 2027. When that date does not exist
/// (one month from 31 January), the period runs to the end of the shorter month instead.
///
/// # Errors
///
/// Fails for `None` coverage (there is nothing to end), for an out-of-range period,
/// and for a date beyond the calendar.
pub fn ends_on(starts_on: NaiveDate, coverage: WarrantyCoverage) -> CalcResult<WarrantyEnd> {
    coverage.validate()?;
    let (value, unit) = match coverage {
        WarrantyCoverage::None => {
            return Err(CalcError::validation("This item has no warranty."));
        }
        WarrantyCoverage::Lifetime => return Ok(WarrantyEnd::Never),
        WarrantyCoverage::Limited { value, unit } => (value, unit),
    };
    let too_far = || CalcError::validation("The warranty would end beyond the calendar.");
    let end = match unit {
        WarrantyUnit::Days => starts_on
            .checked_add_days(Days::new(u64::from(value)))
            .ok_or_else(too_far)?
            .pred_opt()
            .ok_or_else(too_far)?,
        WarrantyUnit::Weeks => starts_on
            .checked_add_days(Days::new(u64::from(value) * 7))
            .ok_or_else(too_far)?
            .pred_opt()
            .ok_or_else(too_far)?,
        WarrantyUnit::Months | WarrantyUnit::Years => {
            let months = if unit == WarrantyUnit::Years {
                value * 12
            } else {
                value
            };
            let same_day = starts_on
                .checked_add_months(Months::new(months))
                .ok_or_else(too_far)?;
            // chrono moves a missing day (31 February) back to the month's last day; that
            // last day is then covered in full rather than losing one more day.
            if same_day.day() == starts_on.day() {
                same_day.pred_opt().ok_or_else(too_far)?
            } else {
                same_day
            }
        }
    };
    Ok(WarrantyEnd::On(end))
}

/// Where a warranty stands on a given day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum WarrantyClock {
    /// Covered, more than a week to go (or no end).
    Active,
    /// Covered, the last week.
    ExpiringSoon,
    /// The last covered day has passed.
    Expired,
}

/// Days of cover left on `today`, the last day counting as one; zero once it has ended.
#[must_use]
pub fn days_remaining(ends_on: NaiveDate, today: NaiveDate) -> i64 {
    ((ends_on - today).num_days() + 1).max(0)
}

/// Where a warranty ending on `ends_on` (`None`: never) stands on `today`.
#[must_use]
pub fn clock(ends_on: Option<NaiveDate>, today: NaiveDate) -> WarrantyClock {
    match ends_on {
        None => WarrantyClock::Active,
        Some(end) => match days_remaining(end, today) {
            0 => WarrantyClock::Expired,
            left if left <= EXPIRING_SOON_DAYS => WarrantyClock::ExpiringSoon,
            _ => WarrantyClock::Active,
        },
    }
}

/// A warranty period to preview: a coverage starting on a day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcWarrantyRequest {
    /// `YYYY-MM-DD`.
    pub starts_on: String,
    pub coverage: WarrantyCoverage,
}

/// The period a coverage gives from its start day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum CalcWarrantyReply {
    /// No warranty.
    NotCovered,
    /// Covered with no end date.
    Lifetime,
    /// Covered up to and including `ends_on` (`YYYY-MM-DD`).
    #[serde(rename_all = "camelCase")]
    Until { ends_on: String },
}

/// Works out the period a coverage gives from its start day.
///
/// # Errors
///
/// Fails when the start day is not a `YYYY-MM-DD` date or the period is out of range.
pub fn compute(request: &CalcWarrantyRequest) -> CalcResult<CalcWarrantyReply> {
    let starts_on = NaiveDate::parse_from_str(&request.starts_on, "%Y-%m-%d")
        .map_err(|_| CalcError::validation("The start date must be a YYYY-MM-DD date."))?;
    if request.coverage == WarrantyCoverage::None {
        return Ok(CalcWarrantyReply::NotCovered);
    }
    Ok(match ends_on(starts_on, request.coverage)? {
        WarrantyEnd::Never => CalcWarrantyReply::Lifetime,
        WarrantyEnd::On(end) => CalcWarrantyReply::Until {
            ends_on: end.to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("test dates are valid")
    }

    fn limited(value: u32, unit: WarrantyUnit) -> WarrantyCoverage {
        WarrantyCoverage::Limited { value, unit }
    }

    #[test]
    fn a_period_ends_the_day_before_the_same_date_one_period_later() {
        let start = day("2026-11-03");
        let cases = [
            (limited(7, WarrantyUnit::Days), "2026-11-09"),
            (limited(1, WarrantyUnit::Days), "2026-11-03"),
            (limited(3, WarrantyUnit::Weeks), "2026-11-23"),
            (limited(12, WarrantyUnit::Months), "2027-11-02"),
            (limited(2, WarrantyUnit::Years), "2028-11-02"),
        ];
        for (coverage, want) in cases {
            assert_eq!(
                ends_on(start, coverage),
                Ok(WarrantyEnd::On(day(want))),
                "{coverage:?}"
            );
        }
    }

    #[test]
    fn a_missing_day_runs_to_the_end_of_the_shorter_month() {
        let one_month = limited(1, WarrantyUnit::Months);
        assert_eq!(
            ends_on(day("2026-01-31"), one_month),
            Ok(WarrantyEnd::On(day("2026-02-28")))
        );
        assert_eq!(
            ends_on(day("2028-01-31"), one_month),
            Ok(WarrantyEnd::On(day("2028-02-29"))),
            "leap year"
        );
        assert_eq!(
            ends_on(day("2026-01-28"), one_month),
            Ok(WarrantyEnd::On(day("2026-02-27")))
        );
        assert_eq!(
            ends_on(day("2028-02-29"), limited(1, WarrantyUnit::Years)),
            Ok(WarrantyEnd::On(day("2029-02-28"))),
            "29 February has no twin next year"
        );
    }

    #[test]
    fn lifetime_never_ends_and_none_has_no_end() {
        let start = day("2026-11-03");
        assert_eq!(
            ends_on(start, WarrantyCoverage::Lifetime),
            Ok(WarrantyEnd::Never)
        );
        assert!(ends_on(start, WarrantyCoverage::None).is_err());
    }

    #[test]
    fn out_of_range_periods_are_refused() {
        let start = day("2026-11-03");
        assert!(ends_on(start, limited(0, WarrantyUnit::Months)).is_err());
        assert!(ends_on(start, limited(101, WarrantyUnit::Years)).is_err());
        assert!(ends_on(start, limited(36_501, WarrantyUnit::Days)).is_err());
        assert!(ends_on(start, limited(100, WarrantyUnit::Years)).is_ok());
    }

    #[test]
    fn the_clock_counts_the_last_day_as_one() {
        let end = day("2026-11-10");
        assert_eq!(days_remaining(end, day("2026-11-10")), 1);
        assert_eq!(days_remaining(end, day("2026-11-11")), 0);
        assert_eq!(days_remaining(end, day("2026-11-01")), 10);
        assert_eq!(clock(Some(end), day("2026-11-02")), WarrantyClock::Active);
        assert_eq!(
            clock(Some(end), day("2026-11-04")),
            WarrantyClock::ExpiringSoon
        );
        assert_eq!(
            clock(Some(end), day("2026-11-10")),
            WarrantyClock::ExpiringSoon
        );
        assert_eq!(clock(Some(end), day("2026-11-11")), WarrantyClock::Expired);
        assert_eq!(clock(None, day("2099-01-01")), WarrantyClock::Active);
    }

    #[test]
    fn the_preview_names_the_period() {
        let request = |coverage| CalcWarrantyRequest {
            starts_on: "2026-11-03".to_string(),
            coverage,
        };
        assert_eq!(
            compute(&request(WarrantyCoverage::None)),
            Ok(CalcWarrantyReply::NotCovered)
        );
        assert_eq!(
            compute(&request(WarrantyCoverage::Lifetime)),
            Ok(CalcWarrantyReply::Lifetime)
        );
        assert_eq!(
            compute(&request(limited(6, WarrantyUnit::Months))),
            Ok(CalcWarrantyReply::Until {
                ends_on: "2027-05-02".to_string()
            })
        );
        let bad = CalcWarrantyRequest {
            starts_on: "03/11/2026".to_string(),
            coverage: WarrantyCoverage::Lifetime,
        };
        assert!(compute(&bad).is_err());
    }

    #[test]
    fn coverage_round_trips_through_json() {
        let coverage = limited(12, WarrantyUnit::Months);
        let json = serde_json::to_value(coverage).expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({ "kind": "limited", "value": 12, "unit": "months" })
        );
        let back: WarrantyCoverage = serde_json::from_value(json).expect("deserializes");
        assert_eq!(back, coverage);
        assert_eq!(
            serde_json::to_value(WarrantyCoverage::Lifetime).expect("serializes"),
            serde_json::json!({ "kind": "lifetime" })
        );
    }
}
