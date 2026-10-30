use super::existing_row::ExistingRow;
use super::millis::millis;

use crate::domain::sync_v2::ChangeRecord;

use crate::modules::sync::mongo_codec;
use crate::modules::sync::mongo_codec::Encoded;
use crate::modules::sync::resources::ResourceSpec;

use mongodb::bson::Document;

/// Builds the full replacement document: encoded payload fields, the merge
/// columns, and (for an existing row) the derived columns it already holds.
pub(super) fn build_document(
    spec: &ResourceSpec,
    record: &ChangeRecord,
    encoded: Encoded,
    existing: Option<&ExistingRow>,
    effective_ms: i64,
    device_id: &str,
) -> Document {
    let mut document = Document::new();
    let id = existing
        .and_then(|e| e.doc.get_object_id("_id").ok())
        .unwrap_or(encoded.id);
    document.insert("_id", id);
    document.insert("key", record.key.as_str());
    for (field, value) in encoded.fields {
        if field != "key" {
            document.insert(field, value);
        }
    }

    // Derived columns: keep what the row has; a new row starts from the value
    // the sender had (recomputed right after the batch anyway).
    for column in mongo_codec::plain_derived(spec) {
        let kept = existing.and_then(|e| e.doc.get(column)).cloned();
        if let Some(value) = kept.or_else(|| encoded.derived.get(column).cloned()) {
            document.insert(column, value);
        }
    }

    let existing_version = existing.map(|e| e.meta.version).unwrap_or(0);
    document.insert(
        "version",
        existing_version.max(encoded.version).max(record.version),
    );
    let created = existing
        .and_then(|e| e.doc.get_datetime("created_at").ok().copied())
        .or(encoded.created_at)
        .unwrap_or_else(|| millis(effective_ms));
    document.insert("created_at", created);
    document.insert("updated_at", millis(effective_ms));
    document.insert("updated_by_device", device_id);
    if let Some(deleted_at) = encoded.deleted_at {
        document.insert("deleted_at", deleted_at);
    }
    document
}
