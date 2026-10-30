use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The physical/sellable condition of a returned item, independent of why
/// it was returned. Drives which `StockMovementType` (if any)
/// `service::credit_notes::create_credit_note` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ItemCondition {
    /// Unopened/undamaged — goes straight back into sellable stock.
    Resalable,
    /// Broken, faulty, or otherwise unsellable as new — requires an
    /// `ItemDisposition`.
    Damaged,
    /// Works, but opened/used — optionally restocked at a discount.
    OpenBoxDiscount,
    /// Returned but not yet checked — no stock movement happens until this
    /// is resolved to one of the other conditions (out of scope for this
    /// pass, see `CreditNoteStatus::AwaitingResolution`).
    PendingInspection,
}
