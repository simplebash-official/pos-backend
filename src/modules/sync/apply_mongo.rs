// Cloud (MongoDB) side of applying a decided sync change: loading the row's
// merge metadata, writing upserts / tombstones / lifecycle merges, and the two
// resource-specific rules (categories fold their subcategories; serials detect
// a double sale). The decision itself (`core::sync_merge::decide`) is made by
// the caller; this module only performs and describes the write.
//
// Writes always go through the scoped handle, so a push can only touch the
// tenant its device token names. Derived columns (stock, balances, refund
// totals) are never overwritten here - `derived_mongo` recomputes them after
// the batch.

use mongodb::bson::{Bson, DateTime as BsonDateTime, Document, doc, oid::ObjectId};
use serde_json::Value;

use crate::{
    clients::db::Db,
    core::{
        error::{AppError, AppResult},
        sync_merge::{RowMeta, merge_lifecycle},
    },
    domain::sync_v2::{ChangeRecord, ConflictKind, DerivedScope, NewConflict},
    modules::sync::{
        cloud_store::coll,
        mongo_codec::{self, Encoded},
        resources::ResourceSpec,
        service::hydrate,
    },
};

/// An existing stored row: its merge metadata and the raw document.
pub(crate) struct ExistingRow {
    pub meta: RowMeta,
    pub doc: Document,
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

/// Reads an integer stored as either Int32 or Int64.
pub(crate) fn num_i64(doc: &Document, field: &str) -> Option<i64> {
    match doc.get(field)? {
        Bson::Int64(v) => Some(*v),
        Bson::Int32(v) => Some(i64::from(*v)),
        Bson::Double(v) => Some(*v as i64),
        _ => None,
    }
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

/// The unique-valued columns of a resource. A NEW row colliding with another
/// entity on one of these is stored with a device-tagged suffix and reported
/// as a `UniqueViolation` instead of being dropped or blocking the batch.
fn unique_columns(resource: &str) -> &'static [&'static str] {
    match resource {
        "products" => &["sku"],
        "categories" | "suppliers" => &["name"],
        "invoices" => &["invoice_number"],
        "creditNotes" => &["credit_note_number"],
        "repairs" | "printJobs" => &["ticket_number"],
        "productSerials" => &["serial_number"],
        "users" => &["email"],
        _ => &[],
    }
}

fn device_tag(device_id: &str) -> String {
    device_id
        .trim_start_matches("dev_")
        .chars()
        .take(4)
        .collect()
}

fn millis(ms: i64) -> BsonDateTime {
    BsonDateTime::from_millis(ms)
}

/// Adds the derived-scope entries a written change implies.
fn note_scope(spec: &ResourceSpec, payload: Option<&Value>, key: &str, doc: &Document, scope: &mut DerivedScope) {
    let text = |name: &str| payload.and_then(|p| p.get(name)).and_then(Value::as_str);
    match spec.name {
        "products" => {
            if let Ok(id) = doc.get_object_id("_id") {
                scope.product_ids.insert(id.to_hex());
            }
        }
        "stockMovements" => {
            if let Some(id) = text("productId") {
                scope.product_ids.insert(id.to_string());
            }
        }
        "customers" => {
            scope.customer_keys.insert(key.to_string());
        }
        "invoices" => {
            scope.invoice_keys.insert(key.to_string());
            if let Some(customer) = text("customerKey") {
                scope.customer_keys.insert(customer.to_string());
            }
        }
        "payments" => {
            if let Some(invoice) = text("invoiceKey") {
                scope.invoice_keys.insert(invoice.to_string());
            }
        }
        "creditNotes" => {
            if let Some(invoice) = text("invoiceKey") {
                scope.invoice_keys.insert(invoice.to_string());
            }
            if let Some(customer) = text("customerKey") {
                scope.customer_keys.insert(customer.to_string());
            }
        }
        _ => {}
    }
}

/// Builds the full replacement document: encoded payload fields, the merge
/// columns, and (for an existing row) the derived columns it already holds.
fn build_document(
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
    document.insert("version", existing_version.max(encoded.version).max(record.version));
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

/// Renames a colliding unique value on a NEW row, recording the conflict.
async fn resolve_unique_collisions(
    db: &Db,
    spec: &ResourceSpec,
    key: &str,
    document: &mut Document,
    device_id: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<()> {
    let collection = coll(db, spec.table)?;
    for column in unique_columns(spec.name) {
        let Ok(value) = document.get_str(column).map(str::to_string) else {
            continue;
        };
        let clash = collection
            .find_one(doc! { *column: &value, "key": { "$ne": key }, "deleted_at": Bson::Null })
            .await?;
        if clash.is_none() {
            continue;
        }
        let stored = format!("{value}-{}", device_tag(device_id));
        document.insert(*column, stored.as_str());
        conflicts.push(NewConflict {
            kind: ConflictKind::UniqueViolation,
            resource: spec.name.to_string(),
            entity_key: key.to_string(),
            detail: serde_json::json!({
                "column": column,
                "originalValue": value,
                "storedValue": stored,
            }),
        });
    }
    Ok(())
}

/// Writes an upsert change (the change already won its merge decision).
pub(crate) async fn apply_upsert(
    db: &Db,
    spec: &ResourceSpec,
    record: &ChangeRecord,
    effective_ms: i64,
    device_id: &str,
    existing: Option<&ExistingRow>,
    // Outputs of the write: conflicts raised and the derived-recompute scope it touches.
    (conflicts, scope): (&mut Vec<NewConflict>, &mut DerivedScope),
) -> AppResult<WriteOutcome> {
    let payload = record
        .payload
        .as_ref()
        .ok_or_else(|| AppError::validation("upsert without payload"))?;
    let encoded = mongo_codec::encode(spec, payload)?;

    // A serial sold on two devices keeps the earlier sale and raises a conflict.
    if spec.name == "productSerials"
        && let Some(existing) = existing
        && let Some(conflict) = serial_double_sale(existing, payload, &record.key)
    {
        conflicts.push(conflict);
        note_scope(spec, Some(payload), &record.key, &existing.doc, scope);
        return Ok(WriteOutcome::Unchanged);
    }

    let mut document = build_document(spec, record, encoded, existing, effective_ms, device_id);
    let collection = coll(db, spec.table)?;

    match existing {
        Some(_) => {
            collection
                .replace_one(doc! { "key": &record.key }, document.clone())
                .await?;
        }
        None => {
            resolve_unique_collisions(db, spec, &record.key, &mut document, device_id, conflicts)
                .await?;
            if let Err(err) = collection.insert_one(document.clone()).await {
                // `_id` is unique across ALL tenants of a collection. A pushed id that is
                // already taken (by another tenant, or forged) refuses just that change;
                // the message stays generic so it cannot be used to probe other tenants.
                let message = err.to_string();
                if message.contains("E11000") {
                    return Ok(WriteOutcome::Rejected(if message.contains("_id_") {
                        "ID_CONFLICT"
                    } else {
                        "UNIQUE_CONFLICT"
                    }));
                }
                return Err(err.into());
            }
        }
    }

    if spec.name == "categories" {
        fold_subcategories(db, record, payload, effective_ms, device_id).await?;
    }
    if spec.name == "users"
        && payload.get("role").and_then(Value::as_str) == Some("admin")
        && let Some(conflict) = extra_admin_conflict(db, &record.key, conflicts).await?
    {
        conflicts.push(conflict);
    }
    note_scope(spec, Some(payload), &record.key, &document, scope);
    Ok(WriteOutcome::Written)
}

/// The one-admin-per-shop rule is enforced only by the users service create
/// path, which sync bypasses. When two DIFFERENT admins arrive for one shop
/// (created on two devices), both are kept - dropping either would lock a real
/// owner out - and the shop owner is told through a conflict. There is no
/// dedicated conflict kind, so this reuses `UniqueViolation` with
/// `reason = MULTIPLE_ADMINS` (the value that is "unique" is the admin role).
async fn extra_admin_conflict(
    db: &Db,
    key: &str,
    raised: &[NewConflict],
) -> AppResult<Option<NewConflict>> {
    let mut cursor = coll(db, "users")?
        .find(doc! { "role": "admin", "deleted_at": Bson::Null })
        .await?;
    let mut admin_keys = Vec::new();
    while cursor.advance().await? {
        if let Ok(k) = cursor.deserialize_current()?.get_str("key") {
            admin_keys.push(k.to_string());
        }
    }
    admin_keys.sort();
    if admin_keys.len() < 2 || !admin_keys.iter().any(|k| k == key) {
        return Ok(None);
    }
    let same_batch = raised.iter().any(|c| c.resource == "users" && c.entity_key == key);
    let stored = coll(db, crate::modules::sync::cloud_store::CONFLICTS)?
        .count_documents(doc! {
            "kind": ConflictKind::UniqueViolation.as_str(),
            "resource": "users", "entity_key": key, "detail.reason": "MULTIPLE_ADMINS",
        })
        .await?;
    if same_batch || stored > 0 {
        return Ok(None);
    }
    Ok(Some(NewConflict {
        kind: ConflictKind::UniqueViolation,
        resource: "users".to_string(),
        entity_key: key.to_string(),
        detail: serde_json::json!({
            "column": "role", "originalValue": "admin", "storedValue": "admin",
            "reason": "MULTIPLE_ADMINS", "adminKeys": admin_keys,
        }),
    }))
}

/// Serial double-sale rule: an existing SOLD serial receiving a different
/// invoice's sale keeps the earlier sale (returned conflict, no write).
fn serial_double_sale(existing: &ExistingRow, payload: &Value, key: &str) -> Option<NewConflict> {
    let existing_invoice = existing.doc.get_str("invoice_key").ok()?;
    let incoming_invoice = payload.get("invoiceKey").and_then(Value::as_str)?;
    let both_sold = existing.doc.get_str("status").ok() == Some("sold")
        && payload.get("status").and_then(Value::as_str) == Some("sold");
    if !both_sold || existing_invoice == incoming_invoice {
        return None;
    }
    Some(NewConflict {
        kind: ConflictKind::SerialDoubleSold,
        resource: "productSerials".to_string(),
        entity_key: key.to_string(),
        detail: serde_json::json!({
            "serialNumber": existing.doc.get_str("serial_number").unwrap_or_default(),
            "invoiceKeys": [existing_invoice, incoming_invoice],
        }),
    })
}

/// Upserts each subcategory carried by a category payload and tombstones the
/// ones the category no longer lists (the wire folds them into the category).
async fn fold_subcategories(
    db: &Db,
    record: &ChangeRecord,
    payload: &Value,
    effective_ms: i64,
    device_id: &str,
) -> AppResult<()> {
    let subs = coll(db, "subcategories")?;
    let listed: Vec<Value> = payload
        .get("subcategories")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut keep = Vec::new();
    for sub in &listed {
        let encoded = mongo_codec::encode_generic("subcategories", &[], sub)?;
        let key = sub
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("subcategory has no key"))?
            .to_string();
        keep.push(key.clone());

        let existing = subs.find_one(doc! { "key": &key }).await?;
        let mut document = Document::new();
        let id: ObjectId = existing
            .as_ref()
            .and_then(|d| d.get_object_id("_id").ok())
            .unwrap_or(encoded.id);
        document.insert("_id", id);
        document.insert("key", key.as_str());
        for (field, value) in encoded.fields {
            if field != "key" {
                document.insert(field, value);
            }
        }
        document.insert("category_key", record.key.as_str());
        let created = existing
            .as_ref()
            .and_then(|d| d.get_datetime("created_at").ok().copied())
            .or(encoded.created_at)
            .unwrap_or_else(|| millis(effective_ms));
        document.insert("created_at", created);
        document.insert("updated_at", millis(effective_ms));
        document.insert(
            "version",
            existing
                .as_ref()
                .and_then(|d| num_i64(d, "version"))
                .unwrap_or(0)
                .max(encoded.version),
        );
        document.insert("updated_by_device", device_id);
        if let Some(deleted_at) = encoded.deleted_at {
            document.insert("deleted_at", deleted_at);
        }
        if existing.is_some() {
            subs.replace_one(doc! { "key": &key }, document).await?;
        } else {
            subs.insert_one(document).await?;
        }
    }

    subs.update_many(
        doc! {
            "category_key": &record.key,
            "key": { "$nin": keep },
            "deleted_at": Bson::Null,
        },
        doc! { "$set": {
            "deleted_at": millis(effective_ms),
            "updated_at": millis(effective_ms),
            "updated_by_device": device_id,
        }},
    )
    .await?;
    Ok(())
}

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
    let document = build_document(spec, &merged_record, encoded, Some(existing), winning_ms, device_id);
    coll(db, spec.table)?
        .replace_one(doc! { "key": &record.key }, document.clone())
        .await?;
    note_scope(spec, Some(&merged.merged), &record.key, &document, scope);
    Ok(WriteOutcome::Written)
}

/// Test hooks (integration tests round-trip real documents through
/// hydrate -> encode to prove the codec's field tables match the models).
#[doc(hidden)]
pub mod test_support {
    use super::*;

    /// The wire DTO of one stored document.
    pub async fn hydrate_for_test(db: &Db, resource: &str, document: Document) -> AppResult<Value> {
        hydrate(db, resource, vec![document])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::internal("nothing hydrated"))
    }

    /// The stored fields (ordinary + derived + `_id`) a payload would produce.
    pub fn encode_for_test(resource: &str, payload: &Value) -> AppResult<Document> {
        let spec = crate::modules::sync::resources::spec(resource)
            .ok_or_else(|| AppError::internal("unknown resource"))?;
        let encoded = mongo_codec::encode(spec, payload)?;
        let mut all = Document::new();
        all.insert("_id", encoded.id);
        all.extend(encoded.fields);
        all.extend(encoded.derived);
        Ok(all)
    }
}
