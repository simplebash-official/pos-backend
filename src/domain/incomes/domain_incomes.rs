use crate::domain::cash_categories::ReportGroup;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Body for `POST /incomes`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateIncomeRequest {
    /// `YYYY-MM-DD`.
    pub income_date: String,
    pub category_key: String,
    #[serde(default)]
    pub payee_key: Option<String>,
    #[serde(default)]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    /// `bank`, `card` or `online`; `drawer` is refused because that is a cash movement.
    pub received_into: ReceivedInto,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Query for `GET /incomes`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct IncomeListQuery {
    pub search: Option<String>,
    pub category_key: Option<String>,
    pub payee_key: Option<String>,
    /// `ledger` or `drawer`.
    pub source: Option<String>,
    /// `YYYY-MM-DD`, inclusive.
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// A page of the combined list.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IncomeListResponse {
    pub incomes: Vec<IncomeRow>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// Money received, as one row of the list: either a ledger income or a drawer add-cash.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IncomeRow {
    /// The income key, or the cash movement key for a drawer row.
    pub key: String,
    pub source: IncomeSource,
    /// `INC-000001` or the voucher number `CSH-000001`.
    pub number: String,
    /// `YYYY-MM-DD`; for a drawer row the day it was recorded (UTC).
    pub date: String,
    pub category_key: String,
    pub category_name: String,
    pub report_group: ReportGroup,
    /// Who it came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    pub received_into: ReceivedInto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}

/// Whether a row of the list came from the ledger or from the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IncomeSource {
    Ledger,
    Drawer,
}

/// Headline figures for the Income page.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStats {
    /// Everything received this month (UTC), wherever it went.
    pub this_month_cents: i64,
    /// Of that, put into the drawer.
    pub this_month_drawer_cents: i64,
    pub last_month_cents: i64,
}

/// Where the money was received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceivedInto {
    /// The cash drawer. Never entered here: it is an add-cash movement.
    Drawer,
    Bank,
    Card,
    Online,
}

impl ReceivedInto {
    #[must_use]
    /// The name this value has on the wire and in storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            ReceivedInto::Drawer => "drawer",
            ReceivedInto::Bank => "bank",
            ReceivedInto::Card => "card",
            ReceivedInto::Online => "online",
        }
    }

    #[must_use]
    /// Reads a wire name; `None` when it names no variant.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(ReceivedInto::Drawer),
            "bank" => Some(ReceivedInto::Bank),
            "card" => Some(ReceivedInto::Card),
            "online" => Some(ReceivedInto::Online),
            _ => None,
        }
    }
}
