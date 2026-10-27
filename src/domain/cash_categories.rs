// Pure business types for drawer cash categories and the payee directory.
// No I/O, no framework types beyond serde/utoipa derives. Categories are data,
// not code: the shop grows its own list, and every cash movement points at one
// by `key`, so renaming a category rewrites history without touching rows.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::domain::sessions::CashMovementType;

/// How a category is counted in profit and loss. Inherited from the movement
/// type when a category is created, so a cashier is never asked about accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportGroup {
    /// Counts as an expense in profit and loss.
    OperatingExpense,
    /// Supplier stock payments: cost of goods, not an expense.
    Purchases,
    /// Salary, advances, EPF and ETF: staff cost.
    Payroll,
    /// Owner capital and drawings: excluded from profit and loss.
    OwnerEquity,
    /// Cash drops, floats and no-sale opens: excluded from profit and loss.
    Transfer,
    /// Loans received and principal repaid: only the interest part is an expense.
    Liabilities,
    /// Money received that is not a sale (scrap, commission, supplier refunds, interest):
    /// counts as income in profit and loss.
    OtherIncome,
}

impl ReportGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReportGroup::OperatingExpense => "operating_expense",
            ReportGroup::Purchases => "purchases",
            ReportGroup::Payroll => "payroll",
            ReportGroup::OwnerEquity => "owner_equity",
            ReportGroup::Transfer => "transfer",
            ReportGroup::Liabilities => "liabilities",
            ReportGroup::OtherIncome => "other_income",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "operating_expense" => Some(ReportGroup::OperatingExpense),
            "purchases" => Some(ReportGroup::Purchases),
            "payroll" => Some(ReportGroup::Payroll),
            "owner_equity" => Some(ReportGroup::OwnerEquity),
            "transfer" => Some(ReportGroup::Transfer),
            "liabilities" => Some(ReportGroup::Liabilities),
            "other_income" => Some(ReportGroup::OtherIncome),
            _ => None,
        }
    }
}

/// Whether a manager has looked at a category yet. Categories a cashier makes on
/// the spot show immediately with a "New" badge and wait here for review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Approved,
    NeedsReview,
}

impl ReviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::Approved => "approved",
            ReviewStatus::NeedsReview => "needs_review",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "approved" => Some(ReviewStatus::Approved),
            "needs_review" => Some(ReviewStatus::NeedsReview),
            _ => None,
        }
    }
}

/// A category a cash movement can be filed under, for one movement type.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashCategory {
    pub id: String,
    pub key: String,
    pub movement_type: CashMovementType,
    pub name: String,
    /// Optional Sinhala name; search matches both languages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_si: Option<String>,
    pub report_group: ReportGroup,
    /// Shipped with the app (seeded). System categories can be hidden or renamed but not merged away.
    pub is_system: bool,
    pub review_status: ReviewStatus,
    /// Pinned categories always lead the picker, ahead of usage ranking.
    pub pinned: bool,
    pub sort_order: i64,
    /// Hidden categories leave the picker but stay on old movements and reports.
    pub active: bool,
    pub requires_receipt: bool,
    /// A payout above this amount needs a manager PIN. `None` means no per-category limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_approval_over_cents: Option<i64>,
    /// How many movements use this category; the picker ranks by this. Counted at read time.
    pub usage_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_name_snapshot: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body for `POST /cash-categories`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCashCategoryRequest {
    pub movement_type: CashMovementType,
    pub name: String,
    #[serde(default)]
    pub name_si: Option<String>,
    /// Only a manager or owner may choose; everyone else inherits it from the movement type.
    #[serde(default)]
    pub report_group: Option<ReportGroup>,
}

/// Body for `PATCH /cash-categories/{key}`. Absent fields are left unchanged.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCashCategoryRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// `Some(None)` is not expressible in JSON here, so clearing the Sinhala name sends an empty string.
    #[serde(default)]
    pub name_si: Option<String>,
    #[serde(default)]
    pub report_group: Option<ReportGroup>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub requires_receipt: Option<bool>,
    /// Zero clears the limit.
    #[serde(default)]
    pub requires_approval_over_cents: Option<i64>,
    /// Marks a cashier-made category as looked at.
    #[serde(default)]
    pub approve: Option<bool>,
}

/// Body for `POST /cash-categories/{key}/merge`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MergeCashCategoryRequest {
    pub target_key: String,
}

/// Query for `GET /cash-categories`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CashCategoryListQuery {
    pub movement_type: Option<String>,
    /// Hidden categories are left out unless this is true (the management screen).
    pub include_hidden: Option<bool>,
    /// Matches name or Sinhala name, ignoring case and spacing.
    pub search: Option<String>,
    /// Only categories waiting for a manager's review.
    pub needs_review: Option<bool>,
    /// Only categories that count as a business cost, for the expense ledger.
    pub for_expenses: Option<bool>,
    /// Only categories that count as income, for the income ledger.
    pub for_income: Option<bool>,
}

/// Query for `GET /cash-categories/similar`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SimilarCashCategoryQuery {
    pub movement_type: String,
    pub name: String,
}

/// Who a payee is. Suppliers and staff live in their own modules and are listed
/// here by reference; only `Vendor` and `Other` rows are stored in `payees`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayeeType {
    Supplier,
    Staff,
    /// Landlord or utility company, added on purpose by a manager.
    Vendor,
    /// A name typed once while paying a small expense, saved for next time.
    Other,
}

impl PayeeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PayeeType::Supplier => "supplier",
            PayeeType::Staff => "staff",
            PayeeType::Vendor => "vendor",
            PayeeType::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supplier" => Some(PayeeType::Supplier),
            "staff" => Some(PayeeType::Staff),
            "vendor" => Some(PayeeType::Vendor),
            "other" => Some(PayeeType::Other),
            _ => None,
        }
    }
}

/// One row of the shared payee directory.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Payee {
    /// `sup_…` / `emp_…` for suppliers and staff, `pye_…` for stored payees.
    pub key: String,
    pub payee_type: PayeeType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// How many cash movements name this payee; the combobox puts frequent ones first.
    pub usage_count: i64,
    /// Hidden payees are not offered when paying someone new; their history stays. Suppliers and
    /// staff are always active here: their own pages decide who is listed.
    pub active: bool,
}

/// Body for `POST /payees`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePayeeRequest {
    pub payee_type: PayeeType,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
}

/// Body for `PATCH /payees/{key}`. Absent fields are left unchanged.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePayeeRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// `Some(None)` is not expressible in JSON here, so clearing the phone sends an empty string.
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Query for `GET /payees`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PayeeListQuery {
    pub payee_type: Option<String>,
    pub search: Option<String>,
    /// Also list hidden payees (needs `payees:manage`); the pickers never ask for them.
    pub include_hidden: Option<bool>,
}

/// Search/uniqueness form of a name: lowercase with all whitespace removed, so
/// "Tea  and Refreshments" and "tea and refreshments" are the same category.
pub fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The key of the category named `name` under `movement_type`. Derived from the
/// two, not random, so the same name made on two offline devices (or seeded on a
/// desktop and in the cloud) is one row that sync merges instead of a duplicate.
pub fn category_key(movement_type: CashMovementType, name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(normalize_name(name).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{}_{hex}",
        crate::core::constants::prefixes::CASH_CATEGORY,
        movement_type.as_str()
    )
}

/// The key of a stored payee, derived from its type and name for the same reason as
/// [`category_key`]: the same landlord typed on two offline devices is one row.
pub fn payee_key(payee_type: PayeeType, name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(normalize_name(name).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{}_{hex}",
        crate::core::constants::prefixes::PAYEE,
        payee_type.as_str()
    )
}

/// The key of a regular bill, derived from its category and payee so the same bill made on two
/// offline devices is one row.
pub fn recurring_expense_key(category_key: &str, payee_key: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{category_key}|{}", payee_key.unwrap_or("")).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{hex}",
        crate::core::constants::prefixes::RECURRING_EXPENSE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_normalize_without_case_or_spacing() {
        assert_eq!(
            normalize_name("  Tea and  Refreshments "),
            "teaandrefreshments"
        );
    }

    #[test]
    fn category_key_is_stable_and_per_type() {
        let a = category_key(CashMovementType::PettyExpense, "Fuel");
        assert_eq!(a, category_key(CashMovementType::PettyExpense, " fuel "));
        assert_ne!(a, category_key(CashMovementType::RentAndBills, "Fuel"));
        assert!(a.starts_with("ccat_petty_expense_"));
    }
}
