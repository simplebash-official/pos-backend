#![forbid(unsafe_code)]

use super::ConflictKind;
use super::decide_append_only::decide_append_only;
use super::decision::Decision;
use super::incoming_change::IncomingChange;
use super::row_meta::RowMeta;
use crate::domain::sync_v2::ChangeOp;
use crate::modules::sync::resources::{MergeClass, ResourceSpec, spec as resource_spec};
use serde_json::Value;

/// Decides the fate of `incoming` against the receiver's `local` row meta.
///
/// Rules, in order: (1) malformed / unknown -> reject; (2) no local row ->
/// apply; (3) append-only -> duplicate; (4) lifecycle -> merge lifecycle
/// columns (an identical repeat is a duplicate); (5) LWW on the tuple
/// `(updated_at_ms, device_id)`, deletes included.
pub fn decide(spec: &ResourceSpec, local: Option<&RowMeta>, incoming: &IncomingChange) -> Decision {
    let rec = incoming.record;

    // (1) unknown resource / malformed payload / delete of an undeletable resource
    if resource_spec(&rec.resource).map(|s| s.table) != Some(spec.table) {
        return Decision::Reject {
            reason: "UNKNOWN_RESOURCE",
        };
    }
    match rec.op {
        ChangeOp::Upsert => {
            if !rec.payload.as_ref().is_some_and(Value::is_object) {
                return Decision::Reject {
                    reason: "INVALID_PAYLOAD",
                };
            }
        }
        ChangeOp::Delete => {
            if !spec.soft_delete {
                return Decision::Reject {
                    reason: "INVALID_PAYLOAD",
                };
            }
        }
    }

    // (2) new row
    let Some(local) = local else {
        return Decision::Apply;
    };

    let same_write =
        incoming.updated_at_ms == local.updated_at_ms && rec.device_id == local.device_id;

    match spec.class {
        // (3)
        MergeClass::AppendOnly => decide_append_only(spec, Some(local), incoming, false),
        // (4)
        MergeClass::Lifecycle => {
            if same_write {
                Decision::Duplicate
            } else {
                Decision::MergeLifecycle
            }
        }
        // (5)
        MergeClass::Lww | MergeClass::LwwWithDerived => {
            let incoming_key = (incoming.updated_at_ms, rec.device_id.as_str());
            let local_key = (local.updated_at_ms, local.device_id.as_str());
            match incoming_key.cmp(&local_key) {
                std::cmp::Ordering::Equal => Decision::Duplicate,
                std::cmp::Ordering::Greater => Decision::Apply,
                std::cmp::Ordering::Less => Decision::KeepLocal {
                    loser: ConflictKind::LwwLoser,
                },
            }
        }
    }
}
