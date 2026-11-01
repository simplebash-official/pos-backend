use super::stored_shape_probe::{Fault, probe};
use crate::modules::accounts::model::AccountOpeningDocument;
use crate::modules::attachments::model::AttachmentDocument;
use crate::modules::auth::model::LoginSessionDocument;
use crate::modules::balances::model::AdvanceRecoveryDocument;
use crate::modules::balances::model::BalanceOpeningDocument;
use crate::modules::billing::model::CreditNoteDocument;
use crate::modules::billing::model::InvoiceDocument;
use crate::modules::billing::model::PaymentDocument;
use crate::modules::cash_categories::model::CashCategoryDocument;
use crate::modules::cash_settings::model::CashSettingsDocument;
use crate::modules::customers::model::CustomerDocument;
use crate::modules::documents::model::GeneratedDocumentDocument;
use crate::modules::employees::model::EmployeeDocument;
use crate::modules::expenses::model::ExpenseDocument;
use crate::modules::expenses::model::ExpensePaymentDocument;
use crate::modules::imports::model::ImportBatchDocument;
use crate::modules::incomes::model::IncomeDocument;
use crate::modules::inventory::model::CategoryDocument;
use crate::modules::inventory::model::ProductDocument;
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::model::StockMovementDocument;
use crate::modules::inventory::model::SubcategoryDocument;
use crate::modules::labels::model::LabelPrintJobDocument;
use crate::modules::labels::model::LabelTemplateDocument;
use crate::modules::payees::model::PayeeDocument;
use crate::modules::print_jobs::model::PrintJobDocument;
use crate::modules::purchases::model::PurchaseDocument;
use crate::modules::purchases::model::PurchaseReceiptDocument;
use crate::modules::purchases::model::ReceiptChargeDocument;
use crate::modules::purchases::model::ReceiptSettlementDocument;
use crate::modules::purchases::model::SupplierCreditDocument;
use crate::modules::quotations::model::QuotationDocument;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use crate::modules::repairs::model::RepairDocument;
use crate::modules::sequences::model::SequenceCounterDocument;
use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::model::DrawerStateDocument;
use crate::modules::sessions::model::RegisterSessionDocument;
use crate::modules::settings::model::DocumentTemplateSettingsDocument;
use crate::modules::settings::model::ShopProfileDocument;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;
use crate::modules::suppliers::model::SupplierDocument;
use crate::modules::tenants::model::TenantDocument;
use crate::modules::users::model::UserDocument;
use mongodb::bson::Document;

/// A MongoDB collection and the typed document stored in it.
#[derive(Debug, Clone, Copy)]
pub struct StoredCollection {
    pub name: &'static str,
    pub(super) check: fn(&Document) -> Result<(), Fault>,
}

macro_rules! stored {
    ($name:literal, $document:ty) => {
        StoredCollection {
            name: $name,
            check: probe::<$document>,
        }
    };
}

/// Every collection whose documents are read through a typed `*Document` struct.
pub const STORED_COLLECTIONS: &[StoredCollection] = &[
    stored!("account_openings", AccountOpeningDocument),
    stored!("advance_recoveries", AdvanceRecoveryDocument),
    stored!("attachments", AttachmentDocument),
    stored!("balance_openings", BalanceOpeningDocument),
    stored!("cash_categories", CashCategoryDocument),
    stored!("cash_movements", CashMovementDocument),
    stored!("cash_settings", CashSettingsDocument),
    stored!("categories", CategoryDocument),
    stored!("credit_notes", CreditNoteDocument),
    stored!("customers", CustomerDocument),
    stored!(
        "document_template_settings",
        DocumentTemplateSettingsDocument
    ),
    stored!("drawer_state", DrawerStateDocument),
    stored!("employees", EmployeeDocument),
    stored!("expense_payments", ExpensePaymentDocument),
    stored!("expenses", ExpenseDocument),
    stored!("generated_documents", GeneratedDocumentDocument),
    stored!("import_batches", ImportBatchDocument),
    stored!("incomes", IncomeDocument),
    stored!("invoices", InvoiceDocument),
    stored!("label_print_jobs", LabelPrintJobDocument),
    stored!("label_templates", LabelTemplateDocument),
    stored!("login_sessions", LoginSessionDocument),
    stored!("payees", PayeeDocument),
    stored!("payments", PaymentDocument),
    stored!("print_jobs", PrintJobDocument),
    stored!("product_serials", ProductSerialDocument),
    stored!("products", ProductDocument),
    stored!("purchase_receipts", PurchaseReceiptDocument),
    stored!("purchases", PurchaseDocument),
    stored!("quotations", QuotationDocument),
    stored!("receipt_charges", ReceiptChargeDocument),
    stored!("receipt_settlements", ReceiptSettlementDocument),
    stored!("recurring_expenses", RecurringExpenseDocument),
    stored!("register_sessions", RegisterSessionDocument),
    stored!("repairs", RepairDocument),
    stored!("sequence_counters", SequenceCounterDocument),
    stored!("shop_profiles", ShopProfileDocument),
    stored!("stock_batches", StockBatchDocument),
    stored!("stock_movements", StockMovementDocument),
    stored!("subcategories", SubcategoryDocument),
    stored!("supplier_credits", SupplierCreditDocument),
    stored!("supplier_products", SupplierProductLinkDocument),
    stored!("suppliers", SupplierDocument),
    stored!("tenants", TenantDocument),
    stored!("users", UserDocument),
];
