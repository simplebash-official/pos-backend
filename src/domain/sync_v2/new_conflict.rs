use super::conflict_kind::ConflictKind;

/// A conflict raised while applying changes or recomputing derived fields;
/// persisted to `sync_conflicts` by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct NewConflict {
    pub kind: ConflictKind,
    pub resource: String,
    pub entity_key: String,
    pub detail: serde_json::Value,
}
