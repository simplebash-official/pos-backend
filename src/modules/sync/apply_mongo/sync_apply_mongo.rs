use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::sync_merge::{RowMeta, merge_lifecycle};
use crate::domain::sync_v2::{ChangeRecord, DerivedScope};
use crate::modules::sync::cloud_store::coll;
use crate::modules::sync::mongo_codec;
use crate::modules::sync::mongo_codec::Encoded;
use crate::modules::sync::resources::ResourceSpec;
use crate::modules::sync::service::hydrate;
use mongodb::bson::{Bson, DateTime as BsonDateTime, Document, doc};
use serde_json::Value;

/// Tombstones a row (soft delete). Deleting an unknown or already-deleted row
/// is a no-op.
pub(crate) async fn apply_delete(
    db: &Db,
    spec: &ResourceSpec,
    record: &ChangeRecord,
    effective_ms: i64,
    device_id: &str,
    existing: Option<&ExistingRow>,
    scope: &mut DerivedScope,
) -> AppResult<WriteOutcome> {
    let Some(existing) = existing else {
        return Ok(WriteOutcome::Unchanged);
    };
    if existing.meta.deleted {
        return Ok(WriteOutcome::Unchanged);
    }
    coll(db, spec.table)?
        .update_one(
            doc! { "key": &record.key },
            doc! { "$set": {
                "deleted_at": millis(effective_ms),
                "updated_at": millis(effective_ms),
                "updated_by_device": device_id,
                "version": existing.meta.version.max(record.version),
            }},
        )
        .await?;
    note_scope(spec, None, &record.key, &existing.doc, scope);
    Ok(WriteOutcome::Written)
}

/// Applies the lifecycle merge for an existing invoice / credit note: only its
/// lifecycle columns can change, the winning status ranked monotonically.
pub(crate) async fn apply_lifecycle(
    db: &Db,
    spec: &ResourceSpec,
    record: &ChangeRecord,
    effective_ms: i64,
    device_id: &str,
    existing: &ExistingRow,
    scope: &mut DerivedScope,
) -> AppResult<WriteOutcome> {
    let incoming = record
        .payload
        .as_ref()
        .ok_or_else(|| AppError::validation("upsert without payload"))?;
    let local = hydrate(db, spec.name, vec![existing.doc.clone()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("existing row could not be hydrated"))?;

    let merged = merge_lifecycle(spec.name, &local, incoming);
    if let Some(reason) = merged.rejected {
        return Ok(WriteOutcome::Rejected(reason));
    }
    if !merged.changed {
        return Ok(WriteOutcome::Unchanged);
    }

    let encoded = mongo_codec::encode(spec, &merged.merged)?;
    let winning_ms = effective_ms.max(existing.meta.updated_at_ms);
    let mut merged_record = record.clone();
    merged_record.payload = Some(merged.merged.clone());
    let document = build_document(
        spec,
        &merged_record,
        encoded,
        Some(existing),
        winning_ms,
        device_id,
    );
    coll(db, spec.table)?
        .replace_one(doc! { "key": &record.key }, document.clone())
        .await?;
    note_scope(spec, Some(&merged.merged), &record.key, &document, scope);
    Ok(WriteOutcome::Written)
}

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

/// An existing stored row: its merge metadata and the raw document.
#[derive(Debug)]
pub(crate) struct ExistingRow {
    pub meta: RowMeta,
    pub doc: Document,
}

fn is_deleted(doc: &Document) -> bool {
    doc.get("deleted_at")
        .is_some_and(|v| !matches!(v, Bson::Null))
}

/// Loads the row with `key` (including tombstones) with its merge metadata.
pub(crate) async fn load_existing(
    db: &Db,
    spec: &ResourceSpec,
    key: &str,
) -> AppResult<Option<ExistingRow>> {
    let Some(doc) = coll(db, spec.table)?.find_one(doc! { "key": key }).await? else {
        return Ok(None);
    };
    let updated_at_ms = doc
        .get_datetime("updated_at")
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);
    let meta = RowMeta {
        updated_at_ms,
        // Web-originated writes carry no device: they sort below any device id
        // at an identical millisecond.
        device_id: doc.get_str("updated_by_device").unwrap_or("").to_string(),
        version: num_i64(&doc, "version").unwrap_or(1),
        deleted: is_deleted(&doc),
    };
    Ok(Some(ExistingRow { meta, doc }))
}

pub(super) fn millis(ms: i64) -> BsonDateTime {
    BsonDateTime::from_millis(ms)
}

/// Adds the derived-scope entries a written change implies.
pub(super) fn note_scope(
    spec: &ResourceSpec,
    payload: Option<&Value>,
    key: &str,
    doc: &Document,
    scope: &mut DerivedScope,
) {
    let text = |camel: &str, snake: &str| {
        payload
            .and_then(|p| p.get(camel))
            .and_then(Value::as_str)
            .or_else(|| doc.get_str(snake).ok())
    };
    match spec.name {
        "products" => {
            if let Ok(id) = doc.get_object_id("_id") {
                scope.product_ids.insert(id.to_hex());
            }
        }
        "stockMovements" => {
            if let Some(id) = text("productId", "product_id") {
                scope.product_ids.insert(id.to_string());
            }
            if let Some(batch) = text("batchKey", "batch_key") {
                scope.batch_keys.insert(batch.to_string());
            }
        }
        "stockBatches" => {
            scope.batch_keys.insert(key.to_string());
        }
        "customers" => {
            scope.customer_keys.insert(key.to_string());
        }
        "invoices" => {
            scope.invoice_keys.insert(key.to_string());
            if let Some(customer) = text("customerKey", "customer_key") {
                scope.customer_keys.insert(customer.to_string());
            }
            if let Some(session) = text("sessionKey", "session_key") {
                scope.session_keys.insert(session.to_string());
            }
        }
        "payments" => {
            if let Some(invoice) = text("invoiceKey", "invoice_key") {
                scope.invoice_keys.insert(invoice.to_string());
            }
            if let Some(session) = text("sessionKey", "session_key") {
                scope.session_keys.insert(session.to_string());
            }
        }
        "creditNotes" => {
            if let Some(invoice) = text("invoiceKey", "invoice_key") {
                scope.invoice_keys.insert(invoice.to_string());
            }
            if let Some(customer) = text("customerKey", "customer_key") {
                scope.customer_keys.insert(customer.to_string());
            }
        }
        "registerSessions" => {
            scope.session_keys.insert(key.to_string());
            scope.drawer_keys.insert("shop".to_string());
            if let Some(cashier) = text("cashierId", "cashier_id") {
                scope.drawer_keys.insert(cashier.to_string());
            }
        }
        "cashMovements" => {
            if let Some(session) = text("sessionKey", "session_key") {
                scope.session_keys.insert(session.to_string());
            } else {
                scope.drawer_keys.insert("shop".to_string());
                if let Some(user) = text("recordedByUserId", "recorded_by_user_id") {
                    scope.drawer_keys.insert(user.to_string());
                }
            }
        }
        "drawerState" => {
            scope.drawer_keys.insert(key.to_string());
        }
        "shopProfiles" => {
            scope.refresh_drawers = true;
        }
        _ => {}
    }
}

/// Reads an integer stored as either Int32 or Int64.
pub(crate) fn num_i64(doc: &Document, field: &str) -> Option<i64> {
    match doc.get(field)? {
        Bson::Int64(v) => Some(*v),
        Bson::Int32(v) => Some(i64::from(*v)),
        Bson::Double(v) => Some(*v as i64),
        _ => None,
    }
}

/// The integer field `field` of a stored document; a missing or non-numeric field is an error.
pub(crate) fn required_i64(doc: &Document, field: &str) -> AppResult<i64> {
    num_i64(doc, field)
        .ok_or_else(|| AppError::internal(format!("stored document has no numeric `{field}`")))
}

/// The string field `field` of a stored document; a missing or non-string field is an error.
pub(crate) fn required_str<'a>(doc: &'a Document, field: &str) -> AppResult<&'a str> {
    doc.get_str(field)
        .map_err(|_| AppError::internal(format!("stored document has no string `{field}`")))
}

/// What performing a write did (drives the push ack).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WriteOutcome {
    Written,
    /// Nothing to do (already in the requested state).
    Unchanged,
    /// The lifecycle merge refused the change.
    Rejected(&'static str),
}
