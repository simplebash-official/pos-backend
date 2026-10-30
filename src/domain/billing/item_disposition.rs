use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What happens to a `Damaged` item. Required whenever `condition ==
/// Damaged`, forbidden otherwise — validated in
/// `service::credit_notes::create_credit_note`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ItemDisposition {
    /// Logged for a supplier RMA/replacement claim — no stock quantity change.
    ReturnToSupplier,
    /// Discarded as inventory loss/shrinkage — no stock quantity change.
    WriteOffScrap,
    /// Held for repair; out of scope for this pass what happens once
    /// repaired (no stock movement is written).
    RepairPending,
}
