#![forbid(unsafe_code)]

use super::strip_for_compare::strip_for_compare;

use crate::modules::sync::resources::ResourceSpec;
use serde_json::Value;

/// True when an append-only row's immutable content differs between two writes.
#[must_use]
pub fn append_only_differs(spec: &ResourceSpec, local: &Value, incoming: &Value) -> bool {
    strip_for_compare(spec, local) != strip_for_compare(spec, incoming)
}
