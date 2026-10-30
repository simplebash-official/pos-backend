#![forbid(unsafe_code)]

use super::decision::Decision;
use super::incoming_change::IncomingChange;
use super::row_meta::RowMeta;

use crate::modules::sync::resources::ResourceSpec;

/// Append-only rule: an existing row is never rewritten. `immutable_differs`
/// is computed by the caller (`append_only_differs`) when the same key arrives
/// from a different write with different content.
#[must_use]
pub fn decide_append_only(
    _spec: &ResourceSpec,
    local: Option<&RowMeta>,
    incoming: &IncomingChange,
    immutable_differs: bool,
) -> Decision {
    let Some(local) = local else {
        return Decision::Apply;
    };
    let same_write = incoming.updated_at_ms == local.updated_at_ms
        && incoming.record.device_id == local.device_id;
    if !same_write && immutable_differs {
        Decision::Reject {
            reason: "IMMUTABLE_MISMATCH",
        }
    } else {
        Decision::Duplicate
    }
}
