//! Stock batches, movements and adjustments, and the low-stock list.

use super::product::PaginationMeta;
use crate::core::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// How many units of one sale/return line came out of (or went back into)
/// one stock batch, and what each of those units cost. A line at one price
/// tier can span several batches (oldest first), so an invoice line carries
/// a list of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchAllocation {
    /// The batch the units came from.
    pub batch_key: String,
    /// Units taken from that batch.
    pub quantity: i64,
    /// Cost of one of those units, in cents (the batch's cost at sale time).
    pub unit_cost_cents: i64,
}

/// Where a stock batch's units came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchSource {
    /// A supplier purchase receipt (`purchase_key` is set).
    Purchase,
    /// Stock a product was created with, or held before batches existed.
    Opening,
    /// A spreadsheet import row.
    Import,
    /// A manual stock-in adjustment.
    Adjustment,
}

impl BatchSource {
    #[must_use]
    /// The name this value has on the wire and in storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Purchase => "purchase",
            Self::Opening => "opening",
            Self::Import => "import",
            Self::Adjustment => "adjustment",
        }
    }
}

impl std::str::FromStr for BatchSource {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "purchase" => Ok(Self::Purchase),
            "opening" => Ok(Self::Opening),
            "import" => Ok(Self::Import),
            "adjustment" => Ok(Self::Adjustment),
            _ => Err(AppError::validation(format!("Invalid batch source: {s}"))),
        }
    }
}

/// One entry in the low-stock report. `deficit` (threshold minus current
/// quantity) is precomputed here so clients don't each reimplement the
/// subtraction.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LowStockItem {
    /// Product MongoDB hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// Product SKU code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// Current stock on hand.
    pub stock_quantity: i64,
    /// Threshold configured for alerts.
    pub min_stock_threshold: i64,
    /// Units below threshold (min_stock_threshold - stock_quantity).
    pub deficit: i64,
}

/// Response for `GET /products/low-stock`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LowStockResponse {
    /// List of products requiring restocking.
    pub items: Vec<LowStockItem>,
    /// Total count of low stock items.
    pub total: u64,
}

/// Body for `PATCH /products/{id}/stock`. `delta` is signed rather than an
/// absolute target quantity, so concurrent adjustments compose instead of
/// racing to overwrite each other's target value. Stock always lives in a
/// batch, so a negative delta names the batch it is taken from (`batchKey`),
/// and a positive delta brings in a new `adjustment` batch, which needs its
/// own cost and selling price.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockAdjustmentRequest {
    /// Signed integer change in stock quantity (+/-).
    pub delta: i64,
    /// Reason or note for manual stock adjustment.
    pub reason: Option<String>,
    /// The batch a negative delta is taken from.
    pub batch_key: Option<String>,
    /// Unit cost of the units a positive delta brings in, in cents.
    pub unit_cost_cents: Option<i64>,
    /// Selling price of the units a positive delta brings in, in cents.
    pub selling_price_cents: Option<i64>,
}

/// Response for a stock adjustment — includes both the before/after
/// quantities so a client can display the change without a second request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockAdjustmentResponse {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// Product SKU code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// New stock level after adjustment.
    pub stock_quantity: i64,
    /// Stock level prior to adjustment.
    pub previous_stock_quantity: i64,
    /// Quantity difference applied (+/-).
    pub delta: i64,
    /// Timestamp of adjustment.
    pub updated_at: DateTime<Utc>,
}

/// One delivery of a product: its own cost, its own selling price and the
/// units of it still on the shelf. Every unit of stock belongs to exactly one
/// batch, and `remaining_quantity` is derived from the stock-movement ledger
/// (the sum of movements carrying this batch's key), exactly like
/// `Product.stock_quantity`, so concurrent offline sales from one batch
/// converge instead of overwriting each other.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockBatch {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key (e.g. stb_...).
    pub key: String,
    /// The product this batch belongs to.
    pub product_key: String,
    /// Where the units came from.
    pub source: BatchSource,
    /// The purchase that received this batch, when `source` is `purchase`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase_key: Option<String>,
    /// The supplier the units came from, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier_key: Option<String>,
    /// Units received into this batch.
    pub received_quantity: i64,
    /// Units still in stock (derived from the movement ledger).
    pub remaining_quantity: i64,
    /// Cost of one unit in cents. Admin only (see `StockBatch::without_cost`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_cost_cents: Option<i64>,
    /// Price one unit of this batch sells at, in cents.
    pub selling_price_cents: i64,
    /// When the batch was received; sales from a tier draw on the oldest first.
    pub received_at: DateTime<Utc>,
    /// Timestamp when the batch was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the batch was last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// ID of client device that last updated this batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl StockBatch {
    /// The batch as a caller who may not see what the shop paid for it gets it.
    #[must_use]
    pub fn without_cost(mut self) -> Self {
        self.unit_cost_cents = None;
        self
    }
}

/// Response for `GET /products/{key}/batches`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockBatchListResponse {
    /// Every batch of the product, newest first (empty ones included, so the
    /// intake history stays visible).
    pub items: Vec<StockBatch>,
}

/// A single recorded stock change (the audit trail entry behind a
/// `StockAdjustmentResponse`). `reference_id` links back to whatever
/// caused the movement (an order, a repair ticket) once other modules
/// start writing these; it's unused (`None`) for manual adjustments.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockMovement {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key of the movement record (e.g. stm_...).
    pub key: String,
    /// Product ID referencing the affected inventory item.
    pub product_id: String,
    /// The stock batch whose units moved. Absent only on the audit-only
    /// return movements (`ReturnWriteOff`/`ReturnSupplierRma`), which move no
    /// quantity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_key: Option<String>,
    /// Number of items added or deducted (+/-).
    pub quantity_delta: i64,
    /// Classification type for the stock movement.
    #[serde(rename = "type")]
    pub movement_type: StockMovementType,
    /// External reference key (e.g. invoice key, purchase key).
    pub reference_id: Option<String>,
    /// Optional explanatory note.
    pub note: Option<String>,
    /// Timestamp when movement occurred.
    pub created_at: DateTime<Utc>,
    /// Timestamp when movement was recorded/updated.
    pub updated_at: DateTime<Utc>,
    /// Version for optimistic locking.
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that recorded this movement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Query params for `GET /inventory/stock-movements`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct StockMovementListQuery {
    /// Filter movements by product ID.
    pub product_id: Option<String>,
    /// Filter movements by movement type.
    pub movement_type: Option<StockMovementType>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}

/// Response for `GET /inventory/stock-movements`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockMovementListResponse {
    /// List of stock movements for current page.
    pub items: Vec<StockMovement>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}

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
    /// The name this value has on the wire and in storage.
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

/// Response for `GET /products/{id}/movements`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StockMovementsResponse {
    /// List of historical stock movements for a specific product.
    pub movements: Vec<StockMovement>,
}

/// Body for `PATCH /batches/{key}`. Either field may be sent alone. Changing
/// the cost is Admin-only (the handler checks it).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStockBatchRequest {
    /// New selling price for the batch's remaining units, in cents.
    pub selling_price_cents: Option<i64>,
    /// Corrected unit cost, in cents (Admin only).
    pub unit_cost_cents: Option<i64>,
}
