use super::encode_generic::encode_generic;
use super::encoded::Encoded;
use super::plain_derived::plain_derived;
use crate::core::error::AppResult;
use crate::modules::sync::resources::ResourceSpec;

use serde_json::Value;

/// Converts a hydrated DTO payload into the parts of a stored document.
pub(crate) fn encode(spec: &ResourceSpec, payload: &Value) -> AppResult<Encoded> {
    encode_generic(spec.name, &plain_derived(spec), payload)
}
