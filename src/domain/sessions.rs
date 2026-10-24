// Pure business types for register sessions (shifts), cash drawer management,
// cash movements, and Z-report reconciliation. No I/O, no framework types
// beyond serde/utoipa derives.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Status of a cash register session / shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Open,
    Closing,
    Closed,
    Reopened,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Open => "open",
            SessionStatus::Closing => "closing",
            SessionStatus::Closed => "closed",
            SessionStatus::Reopened => "reopened",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "open" => Some(SessionStatus::Open),
            "closing" => Some(SessionStatus::Closing),
            "closed" => Some(SessionStatus::Closed),
            "reopened" => Some(SessionStatus::Reopened),
            _ => None,
        }
    }
}

/// What a drawer cash event is. The type decides which fields the form asks for
/// and which way the cash moves (`direction`); the category refines the purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CashMovementType {
    /// Small daily spends (tea, fuel, stationery).
    PettyExpense,
    /// Shop rent, utilities, licences and instalments.
    RentAndBills,
    /// Salary, advances, bonus and statutory remittances.
    StaffPayment,
    /// Stock and supplier dues paid in cash.
    SupplierPayment,
    /// Owner drawings: equity, never an expense.
    OwnerWithdrawal,
    /// Float, capital or a bank withdrawal put into the drawer.
    AddCash,
    /// Excess cash moved to the safe or the bank.
    CashDrop,
    /// Drawer opened without a sale (audit trail only, no amount).
    NoSale,
}

impl CashMovementType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CashMovementType::PettyExpense => "petty_expense",
            CashMovementType::RentAndBills => "rent_and_bills",
            CashMovementType::StaffPayment => "staff_payment",
            CashMovementType::SupplierPayment => "supplier_payment",
            CashMovementType::OwnerWithdrawal => "owner_withdrawal",
            CashMovementType::AddCash => "add_cash",
            CashMovementType::CashDrop => "cash_drop",
            CashMovementType::NoSale => "no_sale",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "petty_expense" => Some(CashMovementType::PettyExpense),
            "rent_and_bills" => Some(CashMovementType::RentAndBills),
            "staff_payment" => Some(CashMovementType::StaffPayment),
            "supplier_payment" => Some(CashMovementType::SupplierPayment),
            "owner_withdrawal" => Some(CashMovementType::OwnerWithdrawal),
            "add_cash" => Some(CashMovementType::AddCash),
            "cash_drop" => Some(CashMovementType::CashDrop),
            "no_sale" => Some(CashMovementType::NoSale),
            _ => None,
        }
    }

    /// Which way cash moves, stored on every row so reports total cash in and
    /// out without parsing type names.
    pub fn direction(&self) -> CashDirection {
        match self {
            CashMovementType::PettyExpense
            | CashMovementType::RentAndBills
            | CashMovementType::StaffPayment
            | CashMovementType::SupplierPayment
            | CashMovementType::OwnerWithdrawal => CashDirection::Out,
            CashMovementType::AddCash => CashDirection::In,
            CashMovementType::CashDrop | CashMovementType::NoSale => CashDirection::Neutral,
        }
    }
}

/// Direction of a movement as far as the drawer's expected cash is concerned.
/// A cash drop is `Neutral` for profit reports but still lowers the drawer, which
/// the session totals track in their own `total_cash_drops_cents` bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CashDirection {
    Out,
    In,
    Neutral,
}

impl CashDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            CashDirection::Out => "out",
            CashDirection::In => "in",
            CashDirection::Neutral => "neutral",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "out" => Some(CashDirection::Out),
            "in" => Some(CashDirection::In),
            "neutral" => Some(CashDirection::Neutral),
            _ => None,
        }
    }
}

/// A register shift session record.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSession {
    pub id: String,
    pub key: String,
    pub session_number: String,
    pub terminal_id: String,
    pub cashier_id: String,
    pub cashier_name_snapshot: String,
    pub status: SessionStatus,
    pub opened_at: DateTime<Utc>,
    pub opening_float_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_denominations: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_note: Option<String>,
    pub opening_discrepancy_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_discrepancy_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_approved_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_approved_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_cashier_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_cashier_name_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_cash_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counted_cash_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_variance_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_denominations: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_float_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_drop_to_safe_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variance_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variance_approved_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variance_approved_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_reason: Option<String>,
    pub total_cash_sales_cents: i64,
    pub total_card_sales_cents: i64,
    pub total_online_sales_cents: i64,
    pub total_credit_sales_cents: i64,
    pub total_cash_refunds_cents: i64,
    pub total_pay_in_cents: i64,
    pub total_pay_out_cents: i64,
    pub total_owner_withdrawals_cents: i64,
    pub total_cash_drops_cents: i64,
    pub has_late_movements: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A recorded movement of cash into or out of the drawer.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashMovement {
    pub id: String,
    pub key: String,
    pub movement_number: String,
    pub movement_type: CashMovementType,
    pub direction: CashDirection,
    pub amount_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    pub terminal_id: String,
    pub category_key: String,
    /// Live name of the category, so a rename shows on old movements.
    pub category_name: String,
    /// A one-off label typed instead of saving a new category; the row then points at the type's "Other".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_label: Option<String>,
    /// `sup_…`, `emp_…` or `pye_…` key from the payee directory, when the payee is a saved one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    /// Who was paid, withdrew or handed over the cash, as written on the voucher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    /// First day of the period a rent, bill or salary covers (`YYYY-MM-DD`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_name_snapshot: Option<String>,
    /// Why a manager let a payout go over the cash in the drawer, or why a movement was reversed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_reason: Option<String>,
    /// Set on a reversal: the movement it cancels. A reversal moves the drawer the opposite way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_key: Option<String>,
    /// Set when a reversal of this movement exists. Worked out when read, never stored on this row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversed_by_key: Option<String>,
    /// Until when the person who recorded this can undo it without a manager. Absent for a reversal,
    /// a no-sale open, or a movement that is already reversed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_until: Option<DateTime<Utc>>,
    pub recorded_late: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_variance_session_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

/// Persistent ledger of cash surviving across sessions for a terminal.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrawerState {
    pub terminal_id: String,
    pub current_balance_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_key: Option<String>,
    pub last_closing_float_cents: i64,
    pub updated_at: DateTime<Utc>,
}

// Request and Response DTOs

/// Body for `POST /billing/sessions/open`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    pub terminal_id: String,
    pub opening_float_cents: i64,
    #[serde(default)]
    pub opening_denominations: Option<serde_json::Value>,
    #[serde(default)]
    pub opening_note: Option<String>,
    #[serde(default)]
    pub discrepancy_reason: Option<String>,
    #[serde(default)]
    pub manager_pin: Option<String>,
}

/// Body for `POST /billing/sessions/{key}/blind-count`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlindCountRequest {
    pub counted_cash_cents: i64,
    #[serde(default)]
    pub closing_denominations: Option<serde_json::Value>,
}

/// Body for `POST /billing/sessions/{key}/close`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionRequest {
    pub counted_cash_cents: i64,
    #[serde(default)]
    pub closing_denominations: Option<serde_json::Value>,
    pub closing_float_cents: i64,
    #[serde(default)]
    pub cash_drop_to_safe_cents: Option<i64>,
    #[serde(default)]
    pub closing_note: Option<String>,
    #[serde(default)]
    pub variance_reason: Option<String>,
    #[serde(default)]
    pub manager_pin: Option<String>,
}

/// Body for `POST /billing/sessions/{key}/reopen`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReopenSessionRequest {
    pub reason: String,
}

/// Body for `POST /billing/cash-movements`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCashMovementRequest {
    pub movement_type: CashMovementType,
    /// Zero for `no_sale`, greater than zero for every other type.
    pub amount_cents: i64,
    /// The open shift this belongs to. Absent only for cash taken out between shifts, which the
    /// drawer ledger carries into the next shift's opening count.
    #[serde(default)]
    pub session_key: Option<String>,
    pub terminal_id: String,
    pub category_key: String,
    #[serde(default)]
    pub custom_label: Option<String>,
    #[serde(default)]
    pub payee_key: Option<String>,
    #[serde(default)]
    pub payee_name: Option<String>,
    #[serde(default)]
    pub period_start: Option<String>,
    #[serde(default)]
    pub period_end: Option<String>,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub manager_pin: Option<String>,
    /// Why a payout bigger than the cash in the drawer is allowed; required together with a manager PIN in that case.
    #[serde(default)]
    pub override_reason: Option<String>,
    /// Set to record a rent or bill payment for a month that is already paid.
    #[serde(default)]
    pub confirm_duplicate: bool,
}

/// Body for `POST /sessions/movements/{key}/reverse`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReverseCashMovementRequest {
    /// Required once the undo window has passed; optional inside it.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Body for `POST /billing/sessions/{key}/convert-variance`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConvertVarianceRequest {
    pub reason: String,
    pub recipient: String,
    pub manager_pin: String,
}

/// Body for `POST /auth/verify-pin`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPinRequest {
    pub pin: String,
    #[serde(default)]
    pub required_role: Option<String>,
}

/// Response for `POST /auth/verify-pin`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPinResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Body for `POST /auth/set-pin`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPinRequest {
    pub pin: String,
    #[serde(default)]
    pub target_user_id: Option<String>,
}

/// Query parameters for listing sessions.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SessionListQuery {
    pub search: Option<String>,
    pub terminal_id: Option<String>,
    pub cashier_id: Option<String>,
    pub status: Option<String>,
    pub date_preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// Paginated list of register sessions.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    pub sessions: Vec<RegisterSession>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// Query parameters for listing cash movements.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CashMovementListQuery {
    pub session_key: Option<String>,
    pub terminal_id: Option<String>,
    pub movement_type: Option<String>,
    pub category_key: Option<String>,
    /// The category plus everything merged into it; filled in by the service from `category_key`.
    #[serde(skip)]
    #[param(ignore)]
    pub category_keys: Option<Vec<String>>,
    pub payee_key: Option<String>,
    pub recorded_late: Option<bool>,
    pub date_preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// Paginated list of cash movements.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashMovementListResponse {
    pub movements: Vec<CashMovement>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// One payment method breakdown row in the Z-Report.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportPaymentMethodSummary {
    pub method: String,
    pub count: u64,
    pub total_cents: i64,
}

/// Cash reconciliation summary for Z-Report.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportCashReconciliation {
    pub opening_float_cents: i64,
    pub cash_sales_cents: i64,
    pub pay_in_cents: i64,
    pub pay_out_cents: i64,
    pub cash_refunds_cents: i64,
    pub owner_withdrawals_cents: i64,
    pub cash_drops_cents: i64,
    pub expected_cash_cents: i64,
    pub counted_cash_cents: i64,
    pub variance_cents: i64,
    pub closing_float_cents: i64,
    pub safe_deposit_cents: i64,
}

/// What one shift took in or paid out under one category, for the Z-report. A reversed movement and its
/// reversal cancel out and are not counted.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportMovementGroup {
    pub movement_type: CashMovementType,
    pub direction: CashDirection,
    pub category_name: String,
    pub count: u64,
    pub total_cents: i64,
}

/// Full Z-Report (End-of-Session Summary).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportResponse {
    pub session: RegisterSession,
    pub gross_sales_cents: i64,
    pub returns_cents: i64,
    pub net_sales_cents: i64,
    pub discount_cents: i64,
    pub total_invoices_count: u64,
    pub payment_methods: Vec<ZReportPaymentMethodSummary>,
    pub cash_movements: Vec<CashMovement>,
    /// The shift's drawer movements totalled by kind and category, in the order the kinds are picked.
    pub movement_summary: Vec<ZReportMovementGroup>,
    pub cash_reconciliation: ZReportCashReconciliation,
    pub generated_at: DateTime<Utc>,
}


