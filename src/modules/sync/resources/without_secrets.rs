use super::secret_fields::secret_fields;

/// A copy of `payload` without the resource's secret fields, for anything that
/// persists a payload outside the resource's own table (conflict details).
#[must_use]
pub fn without_secrets(resource: &str, payload: &serde_json::Value) -> serde_json::Value {
    let mut copy = payload.clone();
    if let Some(object) = copy.as_object_mut() {
        for field in secret_fields(resource) {
            object.remove(*field);
        }
    }
    copy
}
