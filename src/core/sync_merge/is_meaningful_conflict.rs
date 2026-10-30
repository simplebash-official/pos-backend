#![forbid(unsafe_code)]

use super::strip_for_compare::strip_for_compare;

use crate::modules::sync::resources::ResourceSpec;
use serde_json::Value;

/// True when a losing LWW payload differs from the winner in more than
/// timestamps/derived columns, i.e. it is worth showing to the owner.
#[must_use]
pub fn is_meaningful_conflict(spec: &ResourceSpec, winning: &Value, losing: &Value) -> bool {
    strip_for_compare(spec, winning) != strip_for_compare(spec, losing)
}
