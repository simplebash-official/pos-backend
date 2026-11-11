//! Pure business types for register sessions (shifts), cash drawer management,
//! cash movements, and Z-report reconciliation. No I/O, no framework types
//! beyond serde/utoipa derives.

mod domain_sessions;

pub use domain_sessions::ActiveSessionsSummaryResponse;
pub use domain_sessions::CashDirection;
pub use domain_sessions::CashMovement;
pub use domain_sessions::CashMovementListQuery;
pub use domain_sessions::CashMovementListResponse;
pub use domain_sessions::CashMovementType;
pub use domain_sessions::CloseSessionRequest;
pub use domain_sessions::ConvertVarianceRequest;
pub use domain_sessions::CreateCashMovementRequest;
pub use domain_sessions::DENOMINATION_RUPEES;
pub use domain_sessions::DrawerState;
pub use domain_sessions::OpenSessionRequest;
pub use domain_sessions::RegisterSession;
pub use domain_sessions::ReopenSessionRequest;
pub use domain_sessions::ReverseCashMovementRequest;
pub use domain_sessions::SessionListQuery;
pub use domain_sessions::SessionListResponse;
pub use domain_sessions::SessionStatus;
pub use domain_sessions::SetPinRequest;
pub use domain_sessions::SubmitBlindCountRequest;
pub use domain_sessions::VARIANCE_TOLERANCE_CENTS;
pub use domain_sessions::VerifyPinRequest;
pub use domain_sessions::VerifyPinResponse;
pub use domain_sessions::ZReportCashReconciliation;
pub use domain_sessions::ZReportMovementGroup;
pub use domain_sessions::ZReportPaymentMethodSummary;
pub use domain_sessions::ZReportResponse;
