use crate::core::error::AppError;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Why a stock movement happened. Every adjustment (manual or automated)
/// gets recorded as a `StockMovement` tagged with one of these, so the
/// movement history stays auditable even once the originating request is
/// long gone. `Sale`/`PurchaseReceipt`/`ManualAdjustment` are in active use;
/// `RepairPartConsumption` is reserved for when repairs start consuming
/// parts directly. `InvoiceVoidReversal` and the three `Return*` variants
/// used to be a single overloaded `Return` variant — split apart so an
/// invoice-void stock restoration is distinguishable in the audit trail
/// from a credit-note-driven restock/write-off/supplier-RMA.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementType {
    Sale,
    PurchaseReceipt,
    RepairPartConsumption,
    ManualAdjustment,
    /// Stock restored because an invoice was voided (`billing::service::sale::void_invoice`).
    InvoiceVoidReversal,
    /// A credit-note line in `Resalable`/`OpenBoxDiscount` condition put back into sellable stock.
    ReturnRestock,
    /// A credit-note line in `Damaged` condition with disposition `WriteOffScrap` — audit-only, no quantity change.
    ReturnWriteOff,
    /// A credit-note line in `Damaged` condition with disposition `ReturnToSupplier` — audit-only, no quantity change.
    ReturnSupplierRma,
    /// Stock a product started with (or that predates the ledger). Sync treats
    /// stock as the sum of movements, so every unit needs a movement behind it.
    OpeningBalance,
}

impl StockMovementType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::PurchaseReceipt => "purchase_receipt",
            Self::RepairPartConsumption => "repair_part_consumption",
            Self::ManualAdjustment => "manual_adjustment",
            Self::InvoiceVoidReversal => "invoice_void_reversal",
            Self::ReturnRestock => "return_restock",
            Self::ReturnWriteOff => "return_write_off",
            Self::ReturnSupplierRma => "return_supplier_rma",
            Self::OpeningBalance => "opening_balance",
        }
    }
}

impl std::str::FromStr for StockMovementType {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sale" => Ok(Self::Sale),
            "purchase_receipt" => Ok(Self::PurchaseReceipt),
            "repair_part_consumption" => Ok(Self::RepairPartConsumption),
            "manual_adjustment" => Ok(Self::ManualAdjustment),
            "invoice_void_reversal" => Ok(Self::InvoiceVoidReversal),
            "return_restock" => Ok(Self::ReturnRestock),
            "return_write_off" => Ok(Self::ReturnWriteOff),
            "return_supplier_rma" => Ok(Self::ReturnSupplierRma),
            "opening_balance" => Ok(Self::OpeningBalance),
            _ => Err(AppError::validation(format!(
                "Invalid stock movement type: {s}"
            ))),
        }
    }
}
