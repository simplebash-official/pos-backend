use super::stock_movement_type::StockMovementType;

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

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
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that recorded this movement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
