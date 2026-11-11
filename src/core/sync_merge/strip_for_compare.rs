#![forbid(unsafe_code)]

use super::meta_keys::META_KEYS;

use crate::modules::sync::resources::{ResourceSpec, snake_to_camel};
use serde_json::{Map, Value};

/// `payload` minus write metadata, local storage ids, computed fields and the
/// resource's derived columns: the part two replicas must agree on for the same
/// change.
#[must_use]
pub fn strip_for_compare(spec: &ResourceSpec, payload: &Value) -> Value {
    let Value::Object(map) = payload else {
        return payload.clone();
    };
    let derived: Vec<String> = spec.derived.iter().map(|c| snake_to_camel(c)).collect();
    let mut out = Map::new();
    for (k, v) in map {
        if matches!(k.as_str(), "id" | "_id")
            || META_KEYS.contains(&k.as_str())
            || derived.iter().any(|d| d == k)
        {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}
