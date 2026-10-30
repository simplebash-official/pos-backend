use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A credit note's overall resolution state. `AwaitingResolution` is set
/// only when at least one line is `ItemCondition::PendingInspection` — this
/// codebase has no edit-in-place endpoint for any billing document, so
/// there is deliberately no way to move a credit note out of
/// `AwaitingResolution` other than `Voided` in this pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreditNoteStatus {
    Resolved,
    AwaitingResolution,
    Voided,
}
