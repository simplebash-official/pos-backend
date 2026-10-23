// The categories every shop starts with (the "starting point" of the cash movement
// design). They are ordinary rows the shop can rename, hide or reorder; only the
// deterministic key (`domain::cash_categories::category_key`) marks them as the same
// row on every device. Idempotent: an existing row is never overwritten, so a shop's
// edits survive a re-run.

use crate::{
    clients::db::Db,
    core::error::AppResult,
    domain::{cash_categories::ReportGroup, sessions::CashMovementType},
    modules::cash_categories::service,
};

use CashMovementType as T;
use ReportGroup as G;

/// One shipped category: `(type, name, report group)`. Order within a type is its display order.
pub const SYSTEM_CATEGORIES: &[(CashMovementType, &str, ReportGroup)] = &[
    (T::PettyExpense, "Tea and refreshments", G::OperatingExpense),
    (T::PettyExpense, "Transport and delivery", G::OperatingExpense),
    (T::PettyExpense, "Fuel", G::OperatingExpense),
    (T::PettyExpense, "Internet and phone reload", G::OperatingExpense),
    (T::PettyExpense, "Stationery and supplies", G::OperatingExpense),
    (T::PettyExpense, "Cleaning", G::OperatingExpense),
    (T::PettyExpense, "Repairs and maintenance", G::OperatingExpense),
    (T::PettyExpense, "Packaging", G::OperatingExpense),
    (T::PettyExpense, "Courier and postage", G::OperatingExpense),
    (T::PettyExpense, "Other", G::OperatingExpense),
    (T::RentAndBills, "Shop rent", G::OperatingExpense),
    (T::RentAndBills, "Electricity", G::OperatingExpense),
    (T::RentAndBills, "Water", G::OperatingExpense),
    (T::RentAndBills, "Internet and phone", G::OperatingExpense),
    (T::RentAndBills, "Garbage and security", G::OperatingExpense),
    (T::RentAndBills, "Municipal and licence fees", G::OperatingExpense),
    (T::RentAndBills, "Taxes", G::OperatingExpense),
    (T::RentAndBills, "Insurance", G::OperatingExpense),
    (T::RentAndBills, "Loan or lease installment", G::Liabilities),
    (T::RentAndBills, "Software subscriptions", G::OperatingExpense),
    (T::RentAndBills, "Professional fees", G::OperatingExpense),
    (T::RentAndBills, "Bank charges", G::OperatingExpense),
    (T::RentAndBills, "Other", G::OperatingExpense),
    (T::StaffPayment, "Salary", G::Payroll),
    (T::StaffPayment, "Advance", G::Payroll),
    (T::StaffPayment, "Overtime", G::Payroll),
    (T::StaffPayment, "Bonus", G::Payroll),
    (T::StaffPayment, "Commission", G::Payroll),
    (T::StaffPayment, "Allowance", G::Payroll),
    (T::StaffPayment, "Staff loan", G::Payroll),
    (T::StaffPayment, "EPF and ETF remittance", G::Payroll),
    (T::SupplierPayment, "Stock purchase (cash on delivery)", G::Purchases),
    (T::SupplierPayment, "Settle credit balance", G::Purchases),
    (T::SupplierPayment, "Advance or deposit", G::Purchases),
    (T::SupplierPayment, "Freight and handling", G::Purchases),
    (T::SupplierPayment, "Other", G::Purchases),
    (T::OwnerWithdrawal, "Personal use", G::OwnerEquity),
    (T::OwnerWithdrawal, "Profit withdrawal", G::OwnerEquity),
    (T::OwnerWithdrawal, "Reimbursement", G::OwnerEquity),
    (T::OwnerWithdrawal, "Other", G::OwnerEquity),
    (T::AddCash, "Opening float", G::Transfer),
    (T::AddCash, "Change float top-up", G::Transfer),
    (T::AddCash, "Owner capital", G::OwnerEquity),
    (T::AddCash, "Bank withdrawal to drawer", G::Transfer),
    (T::AddCash, "Loan received", G::Liabilities),
    (T::AddCash, "Correction", G::Transfer),
    (T::AddCash, "Scrap and waste sale", G::OtherIncome),
    (T::AddCash, "Commission received", G::OtherIncome),
    (T::AddCash, "Supplier refund", G::OtherIncome),
    (T::AddCash, "Interest received", G::OtherIncome),
    (T::AddCash, "Rental income", G::OtherIncome),
    (T::AddCash, "Other income", G::OtherIncome),
    (T::AddCash, "Other", G::Transfer),
    (T::CashDrop, "To safe", G::Transfer),
    (T::CashDrop, "To bank deposit", G::Transfer),
    (T::CashDrop, "Courier pickup", G::Transfer),
    (T::NoSale, "Give change", G::Transfer),
    (T::NoSale, "Count cash", G::Transfer),
    (T::NoSale, "Check drawer", G::Transfer),
    (T::NoSale, "Test or training", G::Transfer),
    (T::NoSale, "Other", G::Transfer),
];

/// Outcome of a seed run.
#[derive(Debug, Clone)]
pub struct CashCategorySeedResult {
    pub created: usize,
    pub existing: usize,
}

/// Creates any shipped category the shop does not have yet.
pub async fn seed_cash_categories(db: &Db) -> AppResult<CashCategorySeedResult> {
    service::ensure_system_categories(db).await
}
